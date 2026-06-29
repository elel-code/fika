use std::hash::{Hash, Hasher};

use fika_core::{ViewPoint, ViewRect, is_network_path};
use winit::dpi::PhysicalSize;

use crate::shell::context_menu::{
    ShellContextMenu, ShellContextSubmenu, context_menu_items, context_submenu_actions,
};
use crate::shell::create_rename::geometry::{create_dialog_rect_scaled, rename_dialog_rect_scaled};
use crate::shell::drop_menu::{ShellDropMenu, ShellDropTarget, drop_menu_items};
use crate::shell::location::ShellLocationDraft;
use crate::shell::menu_geometry::{
    context_menu_rect_scaled, context_menu_submenu_rect, drop_menu_rect_scaled,
    scaled_context_menu_metric,
};
use crate::shell::metrics::*;
use crate::shell::open_with::ShellOpenWithChooser;
use crate::shell::open_with::geometry::open_with_chooser_rect_scaled;
use crate::shell::options::ShellViewMode;
use crate::shell::pane::{ShellPaneId, ShellPaneProjection, ShellPaneState};
use crate::shell::properties::ShellPropertiesOverlay;
use crate::shell::properties::geometry::properties_overlay_rect_scaled;
use crate::shell::tasks::geometry::task_detail_dialog_rect_scaled;
use crate::{
    FolderPreviewReady, FolderPreviewRoleChange, FolderPreviewRoleKey, ItemPixmapLayout,
    RubberBand, ShellInternalDrag, ShellInternalDragSource, ShellPaneItemTarget, ShellScene,
    folder_preview_role_shell_rect, intersect_rect, pane_content_rect_to_screen,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShellRenderDirtyKey {
    pub(crate) values: Box<[u64]>,
}

impl ShellRenderDirtyKey {
    pub(crate) fn from_scene(scene: &ShellScene, size: PhysicalSize<u32>) -> Self {
        Self::from_scene_with_options(scene, size, ShellRenderDirtyKeyOptions::default())
    }

    pub(crate) fn from_scene_ignoring_hover(scene: &ShellScene, size: PhysicalSize<u32>) -> Self {
        Self::from_scene_with_options(
            scene,
            size,
            ShellRenderDirtyKeyOptions {
                include_hover: false,
                include_context_menu: false,
                include_context_menu_hover: false,
                include_drop_menu: false,
                include_drop_menu_hover: false,
                include_dnd_hover: false,
                include_internal_drag: false,
                include_places_scroll: false,
                include_location_draft: false,
                include_location_changes: false,
                include_properties_overlay_content: false,
                include_properties_overlay_changes: false,
                include_create_dialog_changes: false,
                include_rename_dialog_changes: false,
                include_open_with_chooser_content: false,
                include_open_with_chooser_changes: false,
                include_task_status_changes: false,
                include_rubber_band: false,
                include_folder_preview_roles: true,
            },
        )
    }

    pub(crate) fn from_scene_ignoring_folder_preview_roles(
        scene: &ShellScene,
        size: PhysicalSize<u32>,
    ) -> Self {
        Self::from_scene_with_options(
            scene,
            size,
            ShellRenderDirtyKeyOptions {
                include_folder_preview_roles: false,
                ..ShellRenderDirtyKeyOptions::default()
            },
        )
    }

    pub(crate) fn from_scene_ignoring_hover_and_folder_preview_roles(
        scene: &ShellScene,
        size: PhysicalSize<u32>,
    ) -> Self {
        Self::from_scene_with_options(
            scene,
            size,
            ShellRenderDirtyKeyOptions {
                include_hover: false,
                include_context_menu: false,
                include_context_menu_hover: false,
                include_drop_menu: false,
                include_drop_menu_hover: false,
                include_dnd_hover: false,
                include_internal_drag: false,
                include_places_scroll: false,
                include_location_draft: false,
                include_location_changes: false,
                include_properties_overlay_content: false,
                include_properties_overlay_changes: false,
                include_create_dialog_changes: false,
                include_rename_dialog_changes: false,
                include_open_with_chooser_content: false,
                include_open_with_chooser_changes: false,
                include_task_status_changes: false,
                include_rubber_band: false,
                include_folder_preview_roles: false,
            },
        )
    }

    fn from_scene_with_options(
        scene: &ShellScene,
        size: PhysicalSize<u32>,
        options: ShellRenderDirtyKeyOptions,
    ) -> Self {
        let mut values = Vec::with_capacity(128);
        push_u64(&mut values, size.width as u64);
        push_u64(&mut values, size.height as u64);
        push_f32(&mut values, scene.scale_factor);
        push_u64(&mut values, scene.active_pane.index() as u64);
        push_bool(&mut values, scene.places_visible);
        push_f32(&mut values, scene.places_width);
        push_f32(
            &mut values,
            if options.include_places_scroll {
                scene.places_scroll_y
            } else {
                0.0
            },
        );
        push_bool(&mut values, scene.filter_active);
        push_hash(&mut values, &scene.filter_pattern);
        push_bool(&mut values, scene.show_hidden);
        push_bool(&mut values, scene.dark_mode);
        push_u64(&mut values, scene.zoom_step as i64 as u64);
        push_f32(&mut values, scene.split_pane_left_fraction);
        push_u64(&mut values, scene.item_reflow_dirty_value());

        for pane_id in ShellPaneId::ALL {
            match scene.panes.get(pane_id) {
                Some(pane) => {
                    push_bool(&mut values, true);
                    push_hash(&mut values, &pane.path);
                    push_hash(&mut values, pane.view_mode.as_str());
                    push_u64(&mut values, pane.entries.len() as u64);
                    push_u64(&mut values, pane.dir_count as u64);
                    push_u64(&mut values, pane.filtered_indexes.len() as u64);
                    push_pane_entries_dirty_hash(&mut values, scene, pane_id, pane, size);
                    push_u64(&mut values, pane.selection.len() as u64);
                    push_option_usize(&mut values, pane.selection.anchor);
                    push_option_usize(&mut values, pane.selection.focus);
                    push_hash(&mut values, &pane.selection.selected);
                    push_f32(&mut values, pane.scroll_x);
                    push_f32(&mut values, pane.scroll_y);
                }
                None => push_bool(&mut values, false),
            }
        }
        if options.include_folder_preview_roles {
            push_folder_preview_roles_dirty_hash(&mut values, scene, size);
        }

        push_pane_item_target(
            &mut values,
            if options.include_hover {
                scene.hovered_item
            } else {
                None
            },
        );
        push_option_usize(
            &mut values,
            if options.include_hover {
                scene.hovered_place
            } else {
                None
            },
        );
        push_drop_target(
            &mut values,
            if options.include_dnd_hover {
                scene.dnd_hover_target.as_ref()
            } else {
                None
            },
        );
        push_internal_drag(
            &mut values,
            if options.include_internal_drag {
                scene.internal_drag.as_ref()
            } else {
                None
            },
        );
        push_bool(&mut values, scene.pending_drop_request.is_some());
        push_rubber_band(
            &mut values,
            if options.include_rubber_band {
                scene.rubber_band.as_ref()
            } else {
                None
            },
        );
        push_location_draft(
            &mut values,
            if options.include_location_draft {
                scene.location_draft.as_ref()
            } else {
                None
            },
        );
        push_context_menu(
            &mut values,
            if options.include_context_menu {
                scene.context_menu.as_ref()
            } else {
                None
            },
            options.include_context_menu_hover,
        );
        push_drop_menu(
            &mut values,
            if options.include_drop_menu {
                scene.drop_menu.as_ref()
            } else {
                None
            },
            options.include_drop_menu_hover,
        );
        push_properties_overlay(
            &mut values,
            scene.properties_overlay.as_ref(),
            options.include_properties_overlay_content,
        );
        push_bool(&mut values, scene.create_dialog.is_some());
        push_bool(&mut values, scene.rename_dialog.is_some());
        push_open_with_chooser(
            &mut values,
            scene.open_with_chooser.as_ref(),
            options.include_open_with_chooser_content,
        );
        push_bool(&mut values, scene.trash_conflict_dialog.is_some());
        push_bool(&mut values, scene.task_detail_dialog.is_some());
        push_u64(&mut values, scene.task_statuses.len() as u64);

        for value in [
            if options.include_hover {
                scene.hit_tests
            } else {
                0
            },
            scene.selection_changes,
            scene.context_target_changes,
            scene.context_menu_actions,
            if options.include_properties_overlay_changes {
                scene.properties_changes
            } else {
                0
            },
            if options.include_create_dialog_changes {
                scene.create_changes
            } else {
                0
            },
            if options.include_rename_dialog_changes {
                scene.rename_changes
            } else {
                0
            },
            if options.include_open_with_chooser_changes {
                scene.open_with_changes
            } else {
                0
            },
            scene.open_changes,
            scene.copy_location_changes,
            scene.file_clipboard_changes,
            scene.paste_changes,
            scene.trash_changes,
            scene.places_changes,
            scene.places_resize_changes,
            if options.include_places_scroll {
                scene.places_scroll_changes
            } else {
                0
            },
            scene.keyboard_navigation,
            if options.include_rubber_band {
                scene.rubber_band_updates
            } else {
                0
            },
            scene.view_switches,
            scene.path_changes,
            scene.directory_reloads,
            if options.include_location_changes {
                scene.location_changes
            } else {
                0
            },
            scene.filter_changes,
            scene.hidden_changes,
            scene.zoom_changes,
            scene.split_pane_changes,
            if options.include_dnd_hover {
                scene.dnd_hover_changes
            } else {
                0
            },
            scene.dnd_drop_requests,
            if options.include_task_status_changes {
                scene.task_status_changes
            } else {
                0
            },
        ] {
            push_u64(&mut values, value);
        }

        Self {
            values: values.into_boxed_slice(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ShellRenderDirtyKeyOptions {
    include_hover: bool,
    include_context_menu: bool,
    include_context_menu_hover: bool,
    include_drop_menu: bool,
    include_drop_menu_hover: bool,
    include_dnd_hover: bool,
    include_internal_drag: bool,
    include_places_scroll: bool,
    include_location_draft: bool,
    include_location_changes: bool,
    include_properties_overlay_content: bool,
    include_properties_overlay_changes: bool,
    include_create_dialog_changes: bool,
    include_rename_dialog_changes: bool,
    include_open_with_chooser_content: bool,
    include_open_with_chooser_changes: bool,
    include_task_status_changes: bool,
    include_rubber_band: bool,
    include_folder_preview_roles: bool,
}

impl Default for ShellRenderDirtyKeyOptions {
    fn default() -> Self {
        Self {
            include_hover: true,
            include_context_menu: true,
            include_context_menu_hover: true,
            include_drop_menu: true,
            include_drop_menu_hover: true,
            include_dnd_hover: true,
            include_internal_drag: true,
            include_places_scroll: true,
            include_location_draft: true,
            include_location_changes: true,
            include_properties_overlay_content: true,
            include_properties_overlay_changes: true,
            include_create_dialog_changes: true,
            include_rename_dialog_changes: true,
            include_open_with_chooser_content: true,
            include_open_with_chooser_changes: true,
            include_task_status_changes: true,
            include_rubber_band: true,
            include_folder_preview_roles: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellRenderDamageKind {
    Clean,
    Bounded,
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShellRenderDamage {
    pub(crate) kind: ShellRenderDamageKind,
    pub(crate) bounds: Option<ViewRect>,
    pub(crate) rect_count: usize,
    pub(crate) area_px: f32,
}

impl ShellRenderDamage {
    fn clean() -> Self {
        Self {
            kind: ShellRenderDamageKind::Clean,
            bounds: None,
            rect_count: 0,
            area_px: 0.0,
        }
    }

    pub(crate) fn full(size: PhysicalSize<u32>) -> Self {
        let bounds = full_surface_rect(size);
        Self {
            kind: ShellRenderDamageKind::Full,
            bounds: Some(bounds),
            rect_count: 1,
            area_px: rect_area(bounds),
        }
    }

    fn bounded(rects: Vec<ViewRect>) -> Option<Self> {
        let mut iter = rects.into_iter();
        let first = iter.next()?;
        let mut bounds = first;
        let mut rect_count = 1;
        let mut area_px = rect_area(first);
        for rect in iter {
            bounds = union_rect(bounds, rect);
            rect_count += 1;
            area_px += rect_area(rect);
        }
        Some(Self {
            kind: ShellRenderDamageKind::Bounded,
            bounds: Some(bounds),
            rect_count,
            area_px,
        })
    }

    #[cfg(test)]
    pub(crate) fn between(
        previous: Option<&ShellRenderDamageSnapshot>,
        current: &ShellRenderDamageSnapshot,
        async_results_changed: bool,
    ) -> Self {
        Self::between_with_async_damage(previous, current, async_results_changed, Vec::new())
    }

    pub(crate) fn between_with_async_damage(
        previous: Option<&ShellRenderDamageSnapshot>,
        current: &ShellRenderDamageSnapshot,
        force_full_async_results_changed: bool,
        async_damage_rects: Vec<ViewRect>,
    ) -> Self {
        let Some(previous) = previous else {
            return Self::full(current.size);
        };
        if force_full_async_results_changed || previous.size != current.size {
            return Self::full(current.size);
        }
        if previous.folder_previewless_dirty_key == current.folder_previewless_dirty_key {
            return Self::bounded(async_damage_rects).unwrap_or_else(Self::clean);
        }
        if previous.dirty_key == current.dirty_key {
            return Self::clean();
        }
        if previous.hoverless_dirty_key == current.hoverless_dirty_key {
            let rects = hover_damage_rects(previous, current);
            if let Some(damage) = Self::bounded(rects) {
                return damage;
            }
        }
        if previous.hoverless_folder_previewless_dirty_key
            == current.hoverless_folder_previewless_dirty_key
        {
            let mut rects = hover_damage_rects(previous, current);
            rects.extend(async_damage_rects);
            if let Some(damage) = Self::bounded(rects) {
                return damage;
            }
        }
        Self::full(current.size)
    }

    pub(crate) fn kind_label(self) -> &'static str {
        match self.kind {
            ShellRenderDamageKind::Clean => "clean",
            ShellRenderDamageKind::Bounded => "bounded",
            ShellRenderDamageKind::Full => "full",
        }
    }

    pub(crate) fn scissor_rect(self, size: PhysicalSize<u32>) -> Option<DamageScissorRect> {
        damage_scissor_rect(self.bounds?, size)
    }
}

fn hover_damage_rects(
    previous: &ShellRenderDamageSnapshot,
    current: &ShellRenderDamageSnapshot,
) -> Vec<ViewRect> {
    let mut rects = Vec::new();
    if previous.hovered_item != current.hovered_item {
        push_item_damage_rect(&mut rects, previous, previous.hovered_item);
        push_item_damage_rect(&mut rects, current, current.hovered_item);
    }
    if previous.hovered_place != current.hovered_place {
        push_place_damage_rect(&mut rects, previous, previous.hovered_place);
        push_place_damage_rect(&mut rects, current, current.hovered_place);
    }
    if previous.dnd_hover_target != current.dnd_hover_target {
        push_dnd_hover_damage_rect(&mut rects, previous, previous.dnd_hover_target.as_ref());
        push_dnd_hover_damage_rect(&mut rects, current, current.dnd_hover_target.as_ref());
    }
    push_context_menu_damage_rects(
        &mut rects,
        previous.context_menu.as_ref(),
        current.context_menu.as_ref(),
    );
    push_drop_menu_damage_rects(
        &mut rects,
        previous.drop_menu.as_ref(),
        current.drop_menu.as_ref(),
    );
    if (previous.places_scroll_y - current.places_scroll_y).abs() > f32::EPSILON {
        push_stable_or_changed_damage_rect(
            &mut rects,
            previous.places_sidebar_rect,
            current.places_sidebar_rect,
        );
    }
    push_changed_damage_rect(
        &mut rects,
        previous.drag_preview_rect,
        current.drag_preview_rect,
    );
    push_changed_damage_rect(
        &mut rects,
        previous.rubber_band_rect,
        current.rubber_band_rect,
    );
    push_stable_or_changed_damage_rect(
        &mut rects,
        previous.location_draft_rect,
        current.location_draft_rect,
    );
    push_stable_or_changed_damage_rect(
        &mut rects,
        previous.properties_overlay_rect,
        current.properties_overlay_rect,
    );
    push_stable_or_changed_damage_rect(
        &mut rects,
        previous.create_dialog_rect,
        current.create_dialog_rect,
    );
    push_stable_or_changed_damage_rect(
        &mut rects,
        previous.rename_dialog_rect,
        current.rename_dialog_rect,
    );
    push_stable_or_changed_damage_rect(
        &mut rects,
        previous.open_with_chooser_rect,
        current.open_with_chooser_rect,
    );
    push_stable_or_changed_damage_rect(&mut rects, previous.task_area_rect, current.task_area_rect);
    push_stable_or_changed_damage_rect(
        &mut rects,
        previous.task_detail_dialog_rect,
        current.task_detail_dialog_rect,
    );
    rects
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DamageScissorRect {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct ShellRenderDamageSnapshot {
    pub(crate) dirty_key: ShellRenderDirtyKey,
    hoverless_dirty_key: ShellRenderDirtyKey,
    folder_previewless_dirty_key: ShellRenderDirtyKey,
    hoverless_folder_previewless_dirty_key: ShellRenderDirtyKey,
    hovered_item: Option<ShellPaneItemTarget>,
    hovered_place: Option<usize>,
    dnd_hover_target: Option<ShellDropTarget>,
    item_rects: Vec<(ShellPaneItemTarget, ViewRect)>,
    place_rects: Vec<(usize, ViewRect)>,
    place_gap_rects: Vec<(usize, ViewRect)>,
    pub(crate) places_sidebar_rect: Option<ViewRect>,
    places_scroll_y: f32,
    pub(crate) context_menu: Option<ContextMenuDamageState>,
    pub(crate) drop_menu: Option<DropMenuDamageState>,
    pub(crate) drag_preview_rect: Option<ViewRect>,
    pub(crate) rubber_band_rect: Option<ViewRect>,
    pub(crate) location_draft_rect: Option<ViewRect>,
    pub(crate) properties_overlay_rect: Option<ViewRect>,
    pub(crate) create_dialog_rect: Option<ViewRect>,
    pub(crate) rename_dialog_rect: Option<ViewRect>,
    pub(crate) open_with_chooser_rect: Option<ViewRect>,
    pub(crate) task_area_rect: Option<ViewRect>,
    pub(crate) task_detail_dialog_rect: Option<ViewRect>,
    size: PhysicalSize<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ContextMenuDamageState {
    hovered_row: Option<usize>,
    hovered_submenu_row: Option<usize>,
    active_submenu: Option<ShellContextSubmenu>,
    root_rect: ViewRect,
    pub(crate) overlay_rect: ViewRect,
    root_row_rects: Vec<(usize, ViewRect)>,
    submenu_rect: Option<ViewRect>,
    submenu_row_rects: Vec<(usize, ViewRect)>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DropMenuDamageState {
    hovered_row: Option<usize>,
    pub(crate) overlay_rect: ViewRect,
    row_rects: Vec<(usize, ViewRect)>,
}

impl ShellRenderDamageSnapshot {
    pub(crate) fn from_scene(
        scene: &ShellScene,
        size: PhysicalSize<u32>,
        projections: &[ShellPaneProjection<'_>],
        dirty_key: ShellRenderDirtyKey,
    ) -> Self {
        let mut item_rects = Vec::new();
        for projection in projections {
            for item in &projection.visible_items {
                let Some(entry_index) = projection
                    .view
                    .filtered_indexes
                    .get(item.layout.model_index)
                    .copied()
                else {
                    continue;
                };
                let target = ShellPaneItemTarget {
                    pane: projection.geometry.kind,
                    index: entry_index,
                };
                let rect = pane_content_rect_to_screen(item.layout.visual_rect, projection);
                if let Some(rect) = intersect_rect(rect, projection.geometry.content) {
                    item_rects.push((target, rect));
                }
            }
        }
        let place_rects = if scene.places_visible {
            scene.place_row_rects(size)
        } else {
            Vec::new()
        };
        let place_gap_rects = if scene.places_visible {
            scene.place_gap_rects(size)
        } else {
            Vec::new()
        };
        let places_sidebar_rect = if scene.places_visible {
            let rect = scene.places_sidebar_rect(size);
            (rect.width > 0.0 && rect.height > 0.0).then_some(rect)
        } else {
            None
        };
        let context_menu = scene
            .context_menu
            .as_ref()
            .map(|menu| context_menu_damage_state(menu, size, scene.ui_scale()));
        let drop_menu = scene
            .drop_menu
            .as_ref()
            .map(|menu| drop_menu_damage_state(menu, size, scene.ui_scale()));
        let drag_preview_rect = drag_preview_damage_rect(scene, size);
        let rubber_band_rect = rubber_band_damage_rect(scene, size, projections);
        let location_draft_rect = scene
            .location_draft_pane()
            .and_then(|pane| scene.pane_path_bar_rect(pane, size));
        let properties_overlay_rect = scene
            .properties_overlay
            .as_ref()
            .map(|overlay| properties_overlay_rect_scaled(overlay, size, scene.ui_scale()));
        let create_dialog_rect = scene
            .create_dialog
            .as_ref()
            .map(|dialog| create_dialog_rect_scaled(dialog, size, scene.ui_scale()));
        let rename_dialog_rect = scene
            .rename_dialog
            .as_ref()
            .map(|dialog| rename_dialog_rect_scaled(dialog, size, scene.ui_scale()));
        let open_with_chooser_rect = scene
            .open_with_chooser
            .as_ref()
            .map(|chooser| open_with_chooser_rect_scaled(chooser, size, scene.ui_scale()));
        let task_area_rect = scene.places_task_area_rect(size);
        let task_detail_dialog_rect = scene.task_detail_dialog.as_ref().map(|_| {
            task_detail_dialog_rect_scaled(scene.task_statuses.len(), size, scene.ui_scale())
        });
        Self {
            dirty_key,
            hoverless_dirty_key: ShellRenderDirtyKey::from_scene_ignoring_hover(scene, size),
            folder_previewless_dirty_key:
                ShellRenderDirtyKey::from_scene_ignoring_folder_preview_roles(scene, size),
            hoverless_folder_previewless_dirty_key:
                ShellRenderDirtyKey::from_scene_ignoring_hover_and_folder_preview_roles(scene, size),
            hovered_item: scene.hovered_item,
            hovered_place: scene.hovered_place,
            dnd_hover_target: scene.dnd_hover_target.clone(),
            item_rects,
            place_rects,
            place_gap_rects,
            places_sidebar_rect,
            places_scroll_y: scene.places_scroll_y,
            context_menu,
            drop_menu,
            drag_preview_rect,
            rubber_band_rect,
            location_draft_rect,
            properties_overlay_rect,
            create_dialog_rect,
            rename_dialog_rect,
            open_with_chooser_rect,
            task_area_rect,
            task_detail_dialog_rect,
            size,
        }
    }

    fn item_rect(&self, target: ShellPaneItemTarget) -> Option<ViewRect> {
        self.item_rects
            .iter()
            .find_map(|(candidate, rect)| (*candidate == target).then_some(*rect))
    }

    fn place_rect(&self, index: usize) -> Option<ViewRect> {
        self.place_rects
            .iter()
            .find_map(|(candidate, rect)| (*candidate == index).then_some(*rect))
    }

    fn place_gap_rect(&self, index: usize) -> Option<ViewRect> {
        self.place_gap_rects
            .iter()
            .find_map(|(candidate, rect)| (*candidate == index).then_some(*rect))
    }
}

impl ContextMenuDamageState {
    fn root_row_rect(&self, row: usize) -> Option<ViewRect> {
        self.root_row_rects
            .iter()
            .find_map(|(candidate, rect)| (*candidate == row).then_some(*rect))
    }

    fn submenu_row_rect(&self, row: usize) -> Option<ViewRect> {
        self.submenu_row_rects
            .iter()
            .find_map(|(candidate, rect)| (*candidate == row).then_some(*rect))
    }
}

impl DropMenuDamageState {
    fn row_rect(&self, row: usize) -> Option<ViewRect> {
        self.row_rects
            .iter()
            .find_map(|(candidate, rect)| (*candidate == row).then_some(*rect))
    }
}

fn context_menu_damage_state(
    menu: &ShellContextMenu,
    size: PhysicalSize<u32>,
    scale: f32,
) -> ContextMenuDamageState {
    let root_rect =
        context_menu_shadow_damage_rect(context_menu_rect_scaled(menu, size, scale), size, scale);
    let root_row_rects = (0..context_menu_items(menu).len())
        .filter_map(|row| {
            context_menu_root_row_rect(menu, size, scale, row).map(|rect| (row, rect))
        })
        .collect::<Vec<_>>();
    let submenu_rect = context_menu_submenu_rect(menu, size, scale)
        .map(|rect| context_menu_shadow_damage_rect(rect, size, scale));
    let overlay_rect = submenu_rect
        .map(|submenu_rect| union_rect(root_rect, submenu_rect))
        .unwrap_or(root_rect);
    let submenu_row_rects = menu
        .active_submenu
        .map(|submenu| {
            (0..context_submenu_actions(submenu, menu).len())
                .filter_map(|row| {
                    context_menu_submenu_row_rect(menu, size, scale, row).map(|rect| (row, rect))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    ContextMenuDamageState {
        hovered_row: menu.hovered_row,
        hovered_submenu_row: menu.hovered_submenu_row,
        active_submenu: menu.active_submenu,
        root_rect,
        overlay_rect,
        root_row_rects,
        submenu_rect,
        submenu_row_rects,
    }
}

fn context_menu_root_row_rect(
    menu: &ShellContextMenu,
    size: PhysicalSize<u32>,
    scale: f32,
    row: usize,
) -> Option<ViewRect> {
    (row < context_menu_items(menu).len()).then(|| {
        let rect = context_menu_rect_scaled(menu, size, scale);
        context_menu_row_rect(rect, scale, row)
    })
}

fn context_menu_submenu_row_rect(
    menu: &ShellContextMenu,
    size: PhysicalSize<u32>,
    scale: f32,
    row: usize,
) -> Option<ViewRect> {
    let submenu = menu.active_submenu?;
    (row < context_submenu_actions(submenu, menu).len()).then(|| {
        let rect = context_menu_submenu_rect(menu, size, scale)?;
        Some(context_menu_row_rect(rect, scale, row))
    })?
}

fn context_menu_row_rect(rect: ViewRect, scale: f32, row: usize) -> ViewRect {
    let padding_y = scaled_context_menu_metric(CONTEXT_MENU_VERTICAL_PADDING, scale);
    let row_height = scaled_context_menu_metric(CONTEXT_MENU_ROW_HEIGHT, scale);
    ViewRect {
        x: rect.x,
        y: rect.y + padding_y + row as f32 * row_height,
        width: rect.width,
        height: row_height,
    }
}

fn context_menu_shadow_damage_rect(
    rect: ViewRect,
    size: PhysicalSize<u32>,
    scale_factor: f32,
) -> ViewRect {
    let scale = scale_factor.max(1.0);
    let shadow = ViewRect {
        x: rect.x - (8.0 * scale).round(),
        y: rect.y + (7.0 * scale).round() - (8.0 * scale).round(),
        width: rect.width + (16.0 * scale).round(),
        height: rect.height + (16.0 * scale).round(),
    };
    intersect_rect(shadow, full_surface_rect(size)).unwrap_or(shadow)
}

fn drop_menu_damage_state(
    menu: &ShellDropMenu,
    size: PhysicalSize<u32>,
    scale: f32,
) -> DropMenuDamageState {
    let rect = drop_menu_rect_scaled(menu, size, scale);
    let overlay_rect = context_menu_shadow_damage_rect(rect, size, scale);
    let row_rects = (0..drop_menu_items().len())
        .map(|row| (row, context_menu_row_rect(rect, scale, row)))
        .collect();
    DropMenuDamageState {
        hovered_row: menu.hovered_row,
        overlay_rect,
        row_rects,
    }
}

fn push_item_damage_rect(
    rects: &mut Vec<ViewRect>,
    snapshot: &ShellRenderDamageSnapshot,
    target: Option<ShellPaneItemTarget>,
) {
    if let Some(rect) = target.and_then(|target| snapshot.item_rect(target)) {
        rects.push(rect);
    }
}

fn push_place_damage_rect(
    rects: &mut Vec<ViewRect>,
    snapshot: &ShellRenderDamageSnapshot,
    index: Option<usize>,
) {
    if let Some(rect) = index.and_then(|index| snapshot.place_rect(index)) {
        rects.push(rect);
    }
}

fn push_dnd_hover_damage_rect(
    rects: &mut Vec<ViewRect>,
    snapshot: &ShellRenderDamageSnapshot,
    target: Option<&ShellDropTarget>,
) {
    let rect = match target {
        Some(ShellDropTarget::PaneItem {
            pane,
            index,
            is_dir: true,
            ..
        }) => snapshot.item_rect(ShellPaneItemTarget {
            pane: *pane,
            index: *index,
        }),
        Some(ShellDropTarget::Place { index, .. }) => snapshot.place_rect(*index),
        Some(ShellDropTarget::PlacesGap { index }) => snapshot.place_gap_rect(*index),
        Some(
            ShellDropTarget::PaneItem { is_dir: false, .. }
            | ShellDropTarget::PaneBlank { .. }
            | ShellDropTarget::PlacesBlank,
        )
        | None => None,
    };
    if let Some(rect) = rect {
        rects.push(rect);
    }
}

fn push_context_menu_damage_rects(
    rects: &mut Vec<ViewRect>,
    previous: Option<&ContextMenuDamageState>,
    current: Option<&ContextMenuDamageState>,
) {
    let (Some(previous), Some(current)) = (previous, current) else {
        push_changed_damage_rect(
            rects,
            previous.map(|state| state.overlay_rect),
            current.map(|state| state.overlay_rect),
        );
        return;
    };
    if previous.root_rect != current.root_rect
        || previous.root_row_rects.len() != current.root_row_rects.len()
    {
        push_changed_damage_rect(
            rects,
            Some(previous.overlay_rect),
            Some(current.overlay_rect),
        );
        return;
    }
    if previous.hovered_row != current.hovered_row {
        push_context_menu_root_row_damage(rects, previous, previous.hovered_row);
        push_context_menu_root_row_damage(rects, current, current.hovered_row);
    }
    if previous.active_submenu != current.active_submenu {
        if let Some(rect) = previous.submenu_rect {
            rects.push(rect);
        }
        if let Some(rect) = current.submenu_rect {
            rects.push(rect);
        }
    } else if previous.hovered_submenu_row != current.hovered_submenu_row {
        push_context_menu_submenu_row_damage(rects, previous, previous.hovered_submenu_row);
        push_context_menu_submenu_row_damage(rects, current, current.hovered_submenu_row);
    }
}

fn push_context_menu_root_row_damage(
    rects: &mut Vec<ViewRect>,
    state: &ContextMenuDamageState,
    row: Option<usize>,
) {
    if let Some(rect) = row.and_then(|row| state.root_row_rect(row)) {
        rects.push(rect);
    }
}

fn push_context_menu_submenu_row_damage(
    rects: &mut Vec<ViewRect>,
    state: &ContextMenuDamageState,
    row: Option<usize>,
) {
    if let Some(rect) = row.and_then(|row| state.submenu_row_rect(row)) {
        rects.push(rect);
    }
}

fn push_drop_menu_damage_rects(
    rects: &mut Vec<ViewRect>,
    previous: Option<&DropMenuDamageState>,
    current: Option<&DropMenuDamageState>,
) {
    let (Some(previous), Some(current)) = (previous, current) else {
        push_changed_damage_rect(
            rects,
            previous.map(|state| state.overlay_rect),
            current.map(|state| state.overlay_rect),
        );
        return;
    };
    if previous.overlay_rect != current.overlay_rect
        || previous.row_rects.len() != current.row_rects.len()
    {
        push_changed_damage_rect(
            rects,
            Some(previous.overlay_rect),
            Some(current.overlay_rect),
        );
        return;
    }
    if previous.hovered_row != current.hovered_row {
        push_drop_menu_row_damage(rects, previous, previous.hovered_row);
        push_drop_menu_row_damage(rects, current, current.hovered_row);
    }
}

fn push_drop_menu_row_damage(
    rects: &mut Vec<ViewRect>,
    state: &DropMenuDamageState,
    row: Option<usize>,
) {
    if let Some(rect) = row.and_then(|row| state.row_rect(row)) {
        rects.push(rect);
    }
}

fn push_changed_damage_rect(
    rects: &mut Vec<ViewRect>,
    previous: Option<ViewRect>,
    current: Option<ViewRect>,
) {
    if previous == current {
        return;
    }
    if let Some(rect) = previous {
        rects.push(rect);
    }
    if let Some(rect) = current {
        rects.push(rect);
    }
}

fn push_stable_or_changed_damage_rect(
    rects: &mut Vec<ViewRect>,
    previous: Option<ViewRect>,
    current: Option<ViewRect>,
) {
    match (previous, current) {
        (Some(previous), Some(current)) if previous == current => rects.push(current),
        (previous, current) => push_changed_damage_rect(rects, previous, current),
    }
}

fn drag_preview_damage_rect(scene: &ShellScene, size: PhysicalSize<u32>) -> Option<ViewRect> {
    let drag = scene.internal_drag.as_ref().filter(|drag| drag.active)?;
    let screen = full_surface_rect(size);
    let width = scene
        .scale_metric(188.0)
        .min((screen.width - scene.scale_metric(16.0)).max(1.0));
    let height = scene.scale_metric(42.0);
    let offset = scene.scale_metric(14.0);
    let mut rect = ViewRect {
        x: drag.current.x + offset,
        y: drag.current.y + offset,
        width,
        height,
    };
    rect.x = rect
        .x
        .min((screen.right() - rect.width - scene.scale_metric(8.0)).max(0.0));
    rect.y = rect
        .y
        .min((screen.bottom() - rect.height - scene.scale_metric(8.0)).max(0.0));
    Some(context_menu_shadow_damage_rect(
        rect,
        size,
        scene.ui_scale(),
    ))
}

fn rubber_band_damage_rect(
    scene: &ShellScene,
    size: PhysicalSize<u32>,
    projections: &[ShellPaneProjection<'_>],
) -> Option<ViewRect> {
    let rect = scene
        .rubber_band
        .as_ref()
        .and_then(RubberBand::active_rect)?;
    let projection = projections
        .iter()
        .find(|projection| projection.geometry.kind == scene.active_pane())?;
    let rect = pane_content_rect_to_screen(rect, projection);
    let rect = intersect_rect(rect, projection.geometry.content)?;
    intersect_rect(outset_rect(rect, 1.0), full_surface_rect(size))
}

fn outset_rect(rect: ViewRect, outset: f32) -> ViewRect {
    let outset = outset.max(0.0);
    ViewRect {
        x: rect.x - outset,
        y: rect.y - outset,
        width: rect.width + outset * 2.0,
        height: rect.height + outset * 2.0,
    }
}

pub(crate) fn full_surface_rect(size: PhysicalSize<u32>) -> ViewRect {
    ViewRect {
        x: 0.0,
        y: 0.0,
        width: size.width as f32,
        height: size.height as f32,
    }
}

pub(crate) fn rect_area(rect: ViewRect) -> f32 {
    rect.width.max(0.0) * rect.height.max(0.0)
}

fn union_rect(left: ViewRect, right: ViewRect) -> ViewRect {
    let x = left.x.min(right.x);
    let y = left.y.min(right.y);
    let right_edge = left.right().max(right.right());
    let bottom = left.bottom().max(right.bottom());
    ViewRect {
        x,
        y,
        width: right_edge - x,
        height: bottom - y,
    }
}

pub(crate) fn damage_scissor_rect(
    rect: ViewRect,
    size: PhysicalSize<u32>,
) -> Option<DamageScissorRect> {
    let max_width = size.width.max(1);
    let max_height = size.height.max(1);
    let x = rect.x.floor().max(0.0).min(max_width as f32) as u32;
    let y = rect.y.floor().max(0.0).min(max_height as f32) as u32;
    let right = rect.right().ceil().max(0.0).min(max_width as f32) as u32;
    let bottom = rect.bottom().ceil().max(0.0).min(max_height as f32) as u32;
    (right > x && bottom > y).then_some(DamageScissorRect {
        x,
        y,
        width: right - x,
        height: bottom - y,
    })
}

fn push_u64(values: &mut Vec<u64>, value: u64) {
    values.push(value);
}

fn push_bool(values: &mut Vec<u64>, value: bool) {
    values.push(value as u64);
}

fn push_f32(values: &mut Vec<u64>, value: f32) {
    values.push(value.to_bits() as u64);
}

fn push_hash(values: &mut Vec<u64>, value: impl Hash) {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    values.push(hasher.finish());
}

fn push_pane_entries_dirty_hash(
    values: &mut Vec<u64>,
    scene: &ShellScene,
    pane_id: ShellPaneId,
    pane: &ShellPaneState,
    size: PhysicalSize<u32>,
) {
    if pane.view_mode == ShellViewMode::Details
        && let Some(projection) = scene.pane_projection(pane_id, size)
    {
        push_pane_entries_visual_hash_for_indexes(
            values,
            pane,
            projection
                .visible_items
                .iter()
                .filter_map(|item| pane.filtered_indexes.get(item.layout.model_index).copied()),
        );
        return;
    }
    push_pane_entries_visual_hash(values, pane);
}

fn push_pane_entries_visual_hash(values: &mut Vec<u64>, pane: &ShellPaneState) {
    push_pane_entries_visual_hash_for_indexes(values, pane, pane.filtered_indexes.iter().copied());
}

fn push_pane_entries_visual_hash_for_indexes(
    values: &mut Vec<u64>,
    pane: &ShellPaneState,
    indexes: impl IntoIterator<Item = usize>,
) {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for entry_index in indexes {
        entry_index.hash(&mut hasher);
        match pane.entries.get(entry_index) {
            Some(entry) => {
                true.hash(&mut hasher);
                entry.name.hash(&mut hasher);
                entry.name_width_units.hash(&mut hasher);
                entry.target_path.hash(&mut hasher);
                entry.size_bytes.hash(&mut hasher);
                entry.modified_secs.hash(&mut hasher);
                entry.metadata_complete.hash(&mut hasher);
                entry.mime_type.as_deref().hash(&mut hasher);
                entry.mime_magic_checked.hash(&mut hasher);
                entry.trash_original_path.hash(&mut hasher);
                entry.trash_deletion_time.as_deref().hash(&mut hasher);
                entry.is_dir.hash(&mut hasher);
            }
            None => false.hash(&mut hasher),
        }
    }
    values.push(hasher.finish());
}

fn push_folder_preview_roles_dirty_hash(
    values: &mut Vec<u64>,
    scene: &ShellScene,
    size: PhysicalSize<u32>,
) {
    let roles = scene.folder_preview_roles.borrow();
    let mut states = Vec::new();
    for pane_id in ShellPaneId::ALL {
        let Some(projection) = scene.pane_projection(pane_id, size) else {
            continue;
        };
        for item in &projection.visible_items {
            let Some(entry_index) = projection
                .view
                .filtered_indexes
                .get(item.layout.model_index)
                .copied()
            else {
                continue;
            };
            let Some(entry) = projection.view.entries.get(entry_index) else {
                continue;
            };
            if !entry.is_dir || !entry.metadata_complete {
                continue;
            }
            let Some(modified_secs) = entry.modified_secs else {
                continue;
            };
            let Some(path) = scene.entry_path_for_pane_view(projection.view, entry_index) else {
                continue;
            };
            if is_network_path(&path) {
                continue;
            }
            let pixmap_layout =
                ItemPixmapLayout::from_item_layout(projection.view.view_mode, item.layout);
            let requested_size = scene.folder_preview_role_size_px_for_item(pixmap_layout);
            let requested_key =
                FolderPreviewRoleKey::new(path.clone(), modified_secs, requested_size);
            let state = roles
                .preview_or_closest(&path, modified_secs, requested_size)
                .map(|preview| (1_u8, preview.size_px, preview.stamp))
                .or_else(|| {
                    roles
                        .failed
                        .contains(&requested_key)
                        .then_some((2_u8, requested_size, 0))
                })
                .unwrap_or((0_u8, requested_size, 0));
            states.push((pane_id.index(), path, modified_secs, requested_size, state));
        }
    }
    states.sort();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    states.hash(&mut hasher);
    values.push(hasher.finish());
}

#[cfg(test)]
pub(crate) fn folder_preview_damage_rects_for_changed_keys(
    scene: &ShellScene,
    projections: &[ShellPaneProjection<'_>],
    keys: &[FolderPreviewRoleKey],
) -> Vec<ViewRect> {
    keys.iter()
        .filter_map(|key| folder_preview_damage_rect_for_changed_key(scene, projections, key, None))
        .collect()
}

pub(crate) fn folder_preview_damage_rects_for_changes(
    scene: &ShellScene,
    projections: &[ShellPaneProjection<'_>],
    changes: &[FolderPreviewRoleChange],
) -> Vec<ViewRect> {
    changes
        .iter()
        .filter_map(|change| {
            folder_preview_damage_rect_for_changed_key(
                scene,
                projections,
                &change.key,
                change.previous.as_ref(),
            )
        })
        .collect()
}

fn folder_preview_damage_rect_for_changed_key(
    scene: &ShellScene,
    projections: &[ShellPaneProjection<'_>],
    key: &FolderPreviewRoleKey,
    previous: Option<&FolderPreviewReady>,
) -> Option<ViewRect> {
    for projection in projections {
        for item in &projection.visible_items {
            let Some(entry_index) = projection
                .view
                .filtered_indexes
                .get(item.layout.model_index)
                .copied()
            else {
                continue;
            };
            let Some(entry) = projection.view.entries.get(entry_index) else {
                continue;
            };
            if !entry.is_dir || !entry.metadata_complete {
                continue;
            }
            if entry.modified_secs != Some(key.directory_modified_secs) {
                continue;
            }
            let Some(candidate) = scene.entry_path_for_pane_view(projection.view, entry_index)
            else {
                continue;
            };
            if candidate == key.path {
                let pixmap_layout =
                    ItemPixmapLayout::from_item_layout(projection.view.view_mode, item.layout);
                let requested_size = scene.folder_preview_role_size_px_for_item(pixmap_layout);
                let roles = scene.folder_preview_roles.borrow();
                let preview = roles.preview_or_closest(
                    &key.path,
                    key.directory_modified_secs,
                    requested_size,
                );
                let damage_rect = if preview
                    .filter(|preview| preview.size_px == key.size_px)
                    .is_some()
                {
                    folder_preview_role_shell_rect(pixmap_layout)
                } else if previous.is_some()
                    && preview
                        .map(|preview| preview.size_px != requested_size)
                        .unwrap_or(true)
                {
                    folder_preview_role_shell_rect(pixmap_layout)
                } else if requested_size == key.size_px && roles.failed.contains(key) {
                    folder_preview_role_shell_rect(pixmap_layout)
                } else {
                    continue;
                };
                return Some(pane_content_rect_to_screen(damage_rect, projection));
            }
        }
    }
    None
}

fn push_option_usize(values: &mut Vec<u64>, value: Option<usize>) {
    match value {
        Some(value) => {
            push_bool(values, true);
            push_u64(values, value as u64);
        }
        None => push_bool(values, false),
    }
}

fn push_pane_item_target(values: &mut Vec<u64>, target: Option<ShellPaneItemTarget>) {
    match target {
        Some(target) => {
            push_bool(values, true);
            push_u64(values, target.pane.index() as u64);
            push_u64(values, target.index as u64);
        }
        None => push_bool(values, false),
    }
}

fn push_view_point(values: &mut Vec<u64>, point: ViewPoint) {
    push_f32(values, point.x);
    push_f32(values, point.y);
}

fn push_drop_target(values: &mut Vec<u64>, target: Option<&ShellDropTarget>) {
    match target {
        Some(ShellDropTarget::PaneItem {
            pane,
            index,
            path,
            is_dir,
        }) => {
            push_u64(values, 1);
            push_u64(values, pane.index() as u64);
            push_u64(values, *index as u64);
            push_hash(values, path);
            push_bool(values, *is_dir);
        }
        Some(ShellDropTarget::PaneBlank { pane, path }) => {
            push_u64(values, 2);
            push_u64(values, pane.index() as u64);
            push_hash(values, path);
        }
        Some(ShellDropTarget::Place { index, path }) => {
            push_u64(values, 3);
            push_u64(values, *index as u64);
            push_hash(values, path);
        }
        Some(ShellDropTarget::PlacesGap { index }) => {
            push_u64(values, 4);
            push_u64(values, *index as u64);
        }
        Some(ShellDropTarget::PlacesBlank) => push_u64(values, 5),
        None => push_u64(values, 0),
    }
}

fn push_internal_drag(values: &mut Vec<u64>, drag: Option<&ShellInternalDrag>) {
    match drag {
        Some(drag) => {
            push_bool(values, true);
            match &drag.source {
                ShellInternalDragSource::PaneItem {
                    pane,
                    index,
                    source_path,
                    is_dir,
                } => {
                    push_u64(values, 1);
                    push_u64(values, pane.index() as u64);
                    push_u64(values, *index as u64);
                    push_hash(values, source_path);
                    push_bool(values, *is_dir);
                }
                ShellInternalDragSource::Place { index } => {
                    push_u64(values, 2);
                    push_u64(values, *index as u64);
                }
            }
            push_u64(values, drag.paths.len() as u64);
            for path in &drag.paths {
                push_hash(values, path);
            }
            push_hash(values, &drag.label);
            push_view_point(values, drag.start);
            push_view_point(values, drag.current);
            push_bool(values, drag.active);
        }
        None => push_bool(values, false),
    }
}

fn push_rubber_band(values: &mut Vec<u64>, rubber_band: Option<&RubberBand>) {
    match rubber_band {
        Some(rubber_band) => {
            push_bool(values, true);
            push_view_point(values, rubber_band.start);
            push_view_point(values, rubber_band.current);
            push_bool(values, rubber_band.active);
            push_hash(values, format!("{:?}", rubber_band.mode));
        }
        None => push_bool(values, false),
    }
}

fn push_location_draft(values: &mut Vec<u64>, draft: Option<&ShellLocationDraft>) {
    match draft {
        Some(draft) => {
            push_bool(values, true);
            push_u64(values, draft.pane.index() as u64);
            push_hash(values, &draft.draft.value);
            push_u64(values, draft.draft.cursor as u64);
            push_bool(values, draft.draft.replace_on_insert);
            push_hash(values, format!("{:?}", draft.purpose));
        }
        None => push_bool(values, false),
    }
}

fn push_context_menu(values: &mut Vec<u64>, menu: Option<&ShellContextMenu>, include_hover: bool) {
    match menu {
        Some(menu) => {
            push_bool(values, true);
            push_view_point(values, menu.position);
            if include_hover {
                push_option_usize(values, menu.hovered_row);
                push_option_usize(values, menu.hovered_submenu_row);
                push_hash(values, format!("{:?}", menu.active_submenu));
            }
        }
        None => push_bool(values, false),
    }
}

fn push_drop_menu(values: &mut Vec<u64>, menu: Option<&ShellDropMenu>, include_hover: bool) {
    match menu {
        Some(menu) => {
            push_bool(values, true);
            push_view_point(values, menu.position);
            if include_hover {
                push_option_usize(values, menu.hovered_row);
            }
            push_hash(values, &menu.target_dir);
            push_drop_target(values, Some(&menu.target));
        }
        None => push_bool(values, false),
    }
}

fn push_properties_overlay(
    values: &mut Vec<u64>,
    overlay: Option<&ShellPropertiesOverlay>,
    include_content: bool,
) {
    match overlay {
        Some(overlay) => {
            push_bool(values, true);
            if include_content {
                push_hash(values, &overlay.title);
                push_u64(values, overlay.rows.len() as u64);
                for row in &overlay.rows {
                    push_hash(values, row.label);
                    push_hash(values, &row.value);
                }
            }
        }
        None => push_bool(values, false),
    }
}

fn push_open_with_chooser(
    values: &mut Vec<u64>,
    chooser: Option<&ShellOpenWithChooser>,
    include_content: bool,
) {
    match chooser {
        Some(chooser) => {
            push_bool(values, true);
            if include_content {
                push_hash(values, &chooser.path);
                push_hash(values, &chooser.mime_type);
                push_u64(values, chooser.applications.len() as u64);
                for application in &chooser.applications {
                    push_hash(values, &application.id);
                    push_hash(values, &application.name);
                    push_bool(values, application.is_default);
                }
                for categories in &chooser.application_categories {
                    push_u64(values, categories.len() as u64);
                    for category in categories {
                        push_hash(values, category);
                    }
                }
                push_hash(values, &chooser.query);
                push_u64(values, chooser.expanded_categories.len() as u64);
                for category in &chooser.expanded_categories {
                    push_hash(values, category);
                }
                push_u64(values, chooser.selected_index as u64);
                push_u64(values, chooser.scroll_row as u64);
                push_bool(values, chooser.set_as_default);
                push_hash(values, &chooser.error);
            }
        }
        None => push_bool(values, false),
    }
}
