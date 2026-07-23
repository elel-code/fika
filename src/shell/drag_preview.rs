use crate::platform::PhysicalSize;
use fika_core::{ViewPoint, ViewRect};

use crate::shell::drag_preview_layout::{
    DragPreviewBackgroundStyle, DragPreviewLabelStyle, MultiDragPreviewLayout,
    SingleDragPreviewLayout, multi_drag_preview_layout, pane_single_drag_preview_layout,
    place_single_drag_preview_layout,
};
use crate::shell::file_item_view::style::{
    DolphinItemPalette, item_background_color_for_palette, place_row_background_color_for_palette,
};
use crate::shell::metrics::{PLACES_ICON_SIZE, PLACES_ROW_HEIGHT, PLACES_SIDEBAR_WIDTH};
use crate::shell::render::quad::{QuadVertex, push_clipped_rounded_rect};
use crate::shell::theme::ShellTheme;
use crate::shell::ui_chrome::{push_fallback_file_icon, push_place_icon};
use crate::{
    IconDrawLayer, IconFrameBuilder, ItemPixmapLayout, LabelAlignment, NamedIconFallback,
    ShellInternalDrag, ShellInternalDragSource, ShellScene, ShellViewMode, TEXT_FONT_SIZE,
    TEXT_LINE_HEIGHT, TextFrameBuilder, intersect_rect, pane_item_text_color, place_icon_paint,
};

struct DragPreviewPaintContext<'a> {
    scene: &'a ShellScene,
    clip: ViewRect,
    theme: ShellTheme,
    size: PhysicalSize<u32>,
}

impl ShellScene {
    fn drag_preview_text_width(&self, label: &str) -> f32 {
        let line_height = self.text_line_height();
        let font_size = (TEXT_FONT_SIZE * line_height / TEXT_LINE_HEIGHT).max(1.0);
        self.text_hit_tests
            .borrow_mut()
            .no_wrap_width(label, font_size, line_height)
    }
}

pub(crate) fn push_drag_preview_overlay(
    scene: &ShellScene,
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    icons: &mut IconFrameBuilder<'_>,
    theme: ShellTheme,
    size: PhysicalSize<u32>,
) {
    let Some(drag) = scene.internal_drag.as_ref().filter(|drag| drag.active) else {
        return;
    };
    let screen = surface_rect(size);
    let paint = DragPreviewPaintContext {
        scene,
        clip: screen,
        theme,
        size,
    };

    if drag.paths.len() > 1 {
        let layout = multi_drag_preview_layout(drag.paths.len(), scene.ui_scale());
        push_multi_preview_icons(&paint, vertices, icons, layout, drag);
        return;
    }

    let Some(layout) = single_layout_for_drag(scene, drag, size) else {
        return;
    };
    let origin = ViewRect {
        x: drag.current.x - layout.hotspot.x,
        y: drag.current.y - layout.hotspot.y,
        width: layout.bounds.width,
        height: layout.bounds.height,
    };
    push_single_preview_background(&paint, vertices, origin, layout);
    push_single_preview_icon(&paint, vertices, icons, origin, layout, drag);
    push_single_preview_label(&paint, text, origin, layout, drag, theme);
}

pub(crate) fn drag_preview_damage_rect(
    scene: &ShellScene,
    size: PhysicalSize<u32>,
) -> Option<ViewRect> {
    let drag = scene.internal_drag.as_ref().filter(|drag| drag.active)?;
    let rect = if drag.paths.len() > 1 {
        let layout = multi_drag_preview_layout(drag.paths.len(), scene.ui_scale());
        ViewRect {
            x: drag.current.x - layout.hotspot.x,
            y: drag.current.y - layout.hotspot.y,
            width: layout.bounds.width,
            height: layout.bounds.height,
        }
    } else {
        let layout = single_layout_for_drag(scene, drag, size)?;
        ViewRect {
            x: drag.current.x - layout.hotspot.x,
            y: drag.current.y - layout.hotspot.y,
            width: layout.bounds.width,
            height: layout.bounds.height,
        }
    };
    let inflated = outset_rect(rect, scene.scale_metric(12.0));
    intersect_rect(inflated, surface_rect(size)).or(Some(inflated))
}

fn single_layout_for_drag(
    scene: &ShellScene,
    drag: &ShellInternalDrag,
    size: PhysicalSize<u32>,
) -> Option<SingleDragPreviewLayout> {
    single_layout_for_source(scene, &drag.source, &drag.label, drag.start, size)
}

fn single_layout_for_source(
    scene: &ShellScene,
    source: &ShellInternalDragSource,
    label: &str,
    press_point: ViewPoint,
    size: PhysicalSize<u32>,
) -> Option<SingleDragPreviewLayout> {
    let natural_text_width = scene.drag_preview_text_width(label);
    match source {
        ShellInternalDragSource::PaneItem { pane, index, .. } => {
            let view = scene.pane_view(*pane)?;
            let item_layout = scene.pane_projection(*pane, size).and_then(|projection| {
                let layout_index = projection
                    .view
                    .filtered_indexes
                    .iter()
                    .position(|entry_index| entry_index == index)?;
                projection
                    .visible_items
                    .iter()
                    .find(|item| item.layout.model_index == layout_index)
                    .map(|item| item.layout)
            });
            let icon_size = scene.drag_preview_icon_size_for_pane_item(view, item_layout);
            Some(pane_single_drag_preview_layout(
                view.view_mode,
                item_layout,
                icon_size,
                natural_text_width,
                scene.text_line_height(),
                scene.ui_scale(),
            ))
        }
        ShellInternalDragSource::Place { index } => {
            scene.places.get(*index)?;
            let row = scene
                .place_row_rects(size)
                .into_iter()
                .find(|(row_index, _)| row_index == index)
                .map(|(_, rect)| rect);
            let row_width = row
                .map(|rect| rect.width)
                .unwrap_or_else(|| scene.scale_metric(PLACES_SIDEBAR_WIDTH - 16.0));
            let row_height = scene.scale_metric(PLACES_ROW_HEIGHT);
            // Qt's QListView drag path keeps the pointer's press point in the
            // rendered row as the hotspot. This avoids a visible jump when a
            // place is grabbed away from the row's top-left corner.
            let hotspot = row
                .map(|rect| ViewPoint {
                    x: press_point.x - rect.x,
                    y: press_point.y - rect.y,
                })
                .unwrap_or(ViewPoint {
                    x: row_width / 2.0,
                    y: row_height / 2.0,
                });
            Some(place_single_drag_preview_layout(
                row_width,
                row_height,
                scene.scale_metric(PLACES_ICON_SIZE),
                scene.text_line_height(),
                hotspot,
                scene.ui_scale(),
            ))
        }
    }
}

pub(crate) fn drag_preview_layout_for_source(
    scene: &ShellScene,
    source: &ShellInternalDragSource,
    label: &str,
    press_point: ViewPoint,
    size: PhysicalSize<u32>,
) -> Option<SingleDragPreviewLayout> {
    single_layout_for_source(scene, source, label, press_point, size)
}

fn push_single_preview_background(
    paint: &DragPreviewPaintContext<'_>,
    vertices: &mut Vec<QuadVertex>,
    origin: ViewRect,
    layout: SingleDragPreviewLayout,
) {
    if layout.background_style == DragPreviewBackgroundStyle::None {
        return;
    }
    let rect = ViewRect {
        x: origin.x + layout.background.x,
        y: origin.y + layout.background.y,
        width: layout.background.width,
        height: layout.background.height,
    };
    let color = match layout.background_style {
        DragPreviewBackgroundStyle::SelectedItem => item_background_color_for_palette(
            true,
            false,
            DolphinItemPalette::from_shell_theme(paint.theme),
        ),
        DragPreviewBackgroundStyle::HoveredPlace => place_row_background_color_for_palette(
            false,
            true,
            DolphinItemPalette::from_shell_theme(paint.theme),
        ),
        DragPreviewBackgroundStyle::None => return,
    };
    if layout.radius <= 0.0 {
        crate::shell::render::quad::push_clipped_rect(
            vertices, rect, paint.clip, color, paint.size,
        );
    } else {
        push_clipped_rounded_rect(vertices, rect, paint.clip, layout.radius, color, paint.size);
    }
}

fn push_single_preview_icon(
    paint: &DragPreviewPaintContext<'_>,
    vertices: &mut Vec<QuadVertex>,
    icons: &mut IconFrameBuilder<'_>,
    origin: ViewRect,
    layout: SingleDragPreviewLayout,
    drag: &ShellInternalDrag,
) {
    let icon = ViewRect {
        x: origin.x + layout.icon.x,
        y: origin.y + layout.icon.y,
        width: layout.icon.width,
        height: layout.icon.height,
    };
    push_drag_preview_icon(paint, vertices, icons, icon, drag);
}

fn push_single_preview_label(
    paint: &DragPreviewPaintContext<'_>,
    text: &mut TextFrameBuilder<'_>,
    origin: ViewRect,
    layout: SingleDragPreviewLayout,
    drag: &ShellInternalDrag,
    theme: ShellTheme,
) {
    let Some(label) = layout.label else {
        return;
    };
    let rect = ViewRect {
        x: origin.x + label.rect.x,
        y: origin.y + label.rect.y,
        width: label.rect.width,
        height: label.rect.height,
    };
    let color = match (&drag.source, layout.view_mode) {
        (ShellInternalDragSource::PaneItem { .. }, Some(view_mode)) => {
            let selected_entry = match &drag.source {
                ShellInternalDragSource::PaneItem { pane, index, .. } => {
                    scene_entry(paint.scene, *pane, *index)
                }
                ShellInternalDragSource::Place { .. } => None,
            };
            selected_entry
                .map(|entry| pane_item_text_color(view_mode, entry, true, theme))
                .unwrap_or_else(|| theme.primary_text())
        }
        _ => theme.primary_text(),
    };
    match label.style {
        DragPreviewLabelStyle::FilenameWrapped => {
            text.push_filename_label_wrapped_with_layout(
                &drag.label,
                rect,
                rect,
                paint.clip,
                color,
            );
        }
        DragPreviewLabelStyle::FilenameSingleLine => {
            text.push_filename_label_aligned_no_wrap_with_layout(
                &drag.label,
                rect,
                rect,
                paint.clip,
                color,
                LabelAlignment::Start,
            );
        }
        DragPreviewLabelStyle::PlainSingleLine => {
            text.push_label_aligned_no_wrap(
                &drag.label,
                rect,
                paint.clip,
                color,
                LabelAlignment::Start,
            );
        }
    }
}

fn scene_entry(
    scene: &ShellScene,
    pane: crate::shell::pane::ShellPaneId,
    index: usize,
) -> Option<&crate::Entry> {
    scene.pane_view(pane)?.entries.get(index)
}

fn push_multi_preview_icons(
    paint: &DragPreviewPaintContext<'_>,
    vertices: &mut Vec<QuadVertex>,
    icons: &mut IconFrameBuilder<'_>,
    layout: MultiDragPreviewLayout,
    drag: &ShellInternalDrag,
) {
    let origin = ViewRect {
        x: drag.current.x - layout.hotspot.x,
        y: drag.current.y - layout.hotspot.y,
        width: layout.bounds.width,
        height: layout.bounds.height,
    };
    for index in 0..layout.item_count {
        let Some(cell) = layout.cell_rect(index) else {
            continue;
        };
        let icon = ViewRect {
            x: origin.x + cell.x,
            y: origin.y + cell.y,
            width: cell.width,
            height: cell.height,
        };
        push_drag_preview_path_icon(paint, vertices, icons, icon, drag, index);
    }
}

fn push_drag_preview_path_icon(
    paint: &DragPreviewPaintContext<'_>,
    vertices: &mut Vec<QuadVertex>,
    icons: &mut IconFrameBuilder<'_>,
    icon: ViewRect,
    drag: &ShellInternalDrag,
    ordinal: usize,
) {
    let Some(path) = drag.paths.get(ordinal) else {
        return;
    };
    let ShellInternalDragSource::PaneItem {
        pane,
        index: source_index,
        ..
    } = &drag.source
    else {
        push_drag_preview_placeholder(paint, vertices, icon);
        return;
    };
    let Some(view) = paint.scene.pane_view(*pane) else {
        push_drag_preview_placeholder(paint, vertices, icon);
        return;
    };
    let Some((entry_index, entry)) =
        pane_drag_entry_for_path(paint.scene, view, *source_index, path, ordinal)
    else {
        push_drag_preview_placeholder(paint, vertices, icon);
        return;
    };
    let pixmap_layout = ItemPixmapLayout {
        view_mode: ShellViewMode::Icons,
        icon_rect: icon,
        text_rect: icon,
        text_midline_shift: 0.0,
    };
    let folder_preview =
        paint
            .scene
            .folder_preview_role_for_pane_entry(view, entry_index, pixmap_layout);
    if icons.push_thumbnail_or_icon_on_layer(
        view.path,
        entry,
        folder_preview.as_ref(),
        pixmap_layout,
        paint.clip,
        IconDrawLayer::Overlay,
    ) {
        return;
    }
    push_fallback_file_icon(vertices, entry, icon, paint.clip, paint.theme, paint.size);
}

fn pane_drag_entry_for_path<'a>(
    scene: &ShellScene,
    view: crate::shell::pane::ShellPaneView<'a>,
    source_index: usize,
    path: &std::path::Path,
    ordinal: usize,
) -> Option<(usize, &'a crate::Entry)> {
    if view.selection.contains(source_index)
        && let Some((index, candidate_path)) = view
            .selection
            .selected
            .iter()
            .copied()
            .filter_map(|index| Some((index, scene.entry_path_for_pane_view(view, index)?)))
            .nth(ordinal)
        && candidate_path == path
    {
        return view.entries.get(index).map(|entry| (index, entry));
    }
    view.entries.iter().enumerate().find(|(index, entry)| {
        scene.entry_path_for_pane_view(view, *index).as_deref() == Some(path)
            && !entry.name.is_empty()
    })
}

fn push_drag_preview_placeholder(
    paint: &DragPreviewPaintContext<'_>,
    vertices: &mut Vec<QuadVertex>,
    icon: ViewRect,
) {
    push_clipped_rounded_rect(
        vertices,
        icon,
        paint.clip,
        paint.scene.scale_metric(4.0),
        paint.theme.toolbar_button(true).fill,
        paint.size,
    );
}

fn push_drag_preview_icon(
    paint: &DragPreviewPaintContext<'_>,
    vertices: &mut Vec<QuadVertex>,
    icons: &mut IconFrameBuilder<'_>,
    icon: ViewRect,
    drag: &ShellInternalDrag,
) {
    match &drag.source {
        ShellInternalDragSource::PaneItem { pane, index, .. } => {
            if let Some(view) = paint.scene.pane_view(*pane)
                && let Some(entry) = view.entries.get(*index)
            {
                let pixmap_layout = ItemPixmapLayout {
                    view_mode: view.view_mode,
                    icon_rect: icon,
                    text_rect: icon,
                    text_midline_shift: 0.0,
                };
                let folder_preview =
                    paint
                        .scene
                        .folder_preview_role_for_pane_entry(view, *index, pixmap_layout);
                if icons.push_thumbnail_or_icon_on_layer(
                    view.path,
                    entry,
                    folder_preview.as_ref(),
                    pixmap_layout,
                    paint.clip,
                    IconDrawLayer::Overlay,
                ) {
                    return;
                }
                push_fallback_file_icon(vertices, entry, icon, paint.clip, paint.theme, paint.size);
                return;
            }
        }
        ShellInternalDragSource::Place { index } => {
            if let Some(place) = paint.scene.places.get(*index) {
                let icon_name = if paint.scene.trash_place_has_items(place) {
                    "user-trash-full"
                } else {
                    place.icon_name
                };
                if icons.push_named_theme_icon(
                    icon_name,
                    NamedIconFallback::Service,
                    icon,
                    paint.clip,
                    IconDrawLayer::Overlay,
                ) {
                    return;
                }
                push_place_icon(
                    vertices,
                    icon,
                    paint.clip,
                    place_icon_paint(place),
                    paint.theme,
                    paint.scene.ui_scale(),
                    paint.size,
                );
                return;
            }
        }
    }
    push_clipped_rounded_rect(
        vertices,
        icon,
        paint.clip,
        paint.scene.scale_metric(6.0),
        paint.theme.toolbar_button(true).fill,
        paint.size,
    );
}

fn surface_rect(size: PhysicalSize<u32>) -> ViewRect {
    ViewRect {
        x: 0.0,
        y: 0.0,
        width: size.width.max(1) as f32,
        height: size.height.max(1) as f32,
    }
}

fn outset_rect(rect: ViewRect, outset: f32) -> ViewRect {
    ViewRect {
        x: rect.x - outset,
        y: rect.y - outset,
        width: rect.width + outset * 2.0,
        height: rect.height + outset * 2.0,
    }
}
