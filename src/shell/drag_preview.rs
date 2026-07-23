use crate::platform::PhysicalSize;
use cosmic_text::Color as TextColor;
use fika_core::ViewRect;

use crate::shell::render::quad::{QuadVertex, push_clipped_rounded_rect};
use crate::shell::theme::{ShellTheme, UiColor};
use crate::shell::ui_chrome::{push_fallback_file_icon, push_place_icon};
use crate::{
    DND_PREVIEW_ICON_OUTLINE, DND_PREVIEW_LABEL_HEIGHT, DND_PREVIEW_LABEL_MIN_WIDTH, IconDrawLayer,
    IconFrameBuilder, ItemPixmapLayout, LabelAlignment, NamedIconFallback, PLACES_ICON_SIZE,
    ShellInternalDrag, ShellInternalDragSource, ShellScene, TextFrameBuilder, intersect_rect,
    place_icon_paint, translated_rect,
};

struct DragPreviewPaintContext<'a> {
    scene: &'a ShellScene,
    clip: ViewRect,
    theme: ShellTheme,
    size: PhysicalSize<u32>,
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
    let rect = drag_preview_rect(scene, drag, size);
    let top_icon = drag_preview_top_icon_rect(scene, drag, size);
    let paint = DragPreviewPaintContext {
        scene,
        clip: screen,
        theme,
        size,
    };

    push_deepin_drag_icon_stack(&paint, vertices, icons, top_icon, drag);

    push_deepin_drag_label(&paint, vertices, text, rect, drag);
    if drag.paths.len() > 1 {
        push_deepin_drag_count_badge(
            &paint,
            vertices,
            text,
            top_icon,
            drag.paths.len(),
            theme.drag_preview().badge,
        );
    }
}

pub(crate) fn drag_preview_damage_rect(
    scene: &ShellScene,
    size: PhysicalSize<u32>,
) -> Option<ViewRect> {
    let drag = scene.internal_drag.as_ref().filter(|drag| drag.active)?;
    let rect = drag_preview_rect(scene, drag, size);
    let inflated = outset_rect(rect, scene.scale_metric(12.0));
    intersect_rect(inflated, surface_rect(size)).or(Some(inflated))
}

fn drag_preview_rect(
    scene: &ShellScene,
    drag: &ShellInternalDrag,
    size: PhysicalSize<u32>,
) -> ViewRect {
    let icon_size = drag_preview_icon_size(scene, drag, size);
    let outline = drag_preview_icon_outline(scene);
    let pixmap_size = icon_size + outline * 2.0;
    let has_label = !drag.label.is_empty();
    let label_width = if has_label {
        scene.scale_metric(DND_PREVIEW_LABEL_MIN_WIDTH)
    } else {
        0.0
    };
    let label_height = if has_label {
        scene.scale_metric(DND_PREVIEW_LABEL_HEIGHT)
    } else {
        0.0
    };
    let width = pixmap_size.max(label_width);
    let height = pixmap_size + label_height;
    ViewRect {
        x: drag.current.x - width / 2.0,
        y: drag.current.y - outline - icon_size / 2.0,
        width,
        height,
    }
}

fn drag_preview_top_icon_rect(
    scene: &ShellScene,
    drag: &ShellInternalDrag,
    size: PhysicalSize<u32>,
) -> ViewRect {
    let rect = drag_preview_rect(scene, drag, size);
    let icon_size = drag_preview_icon_size(scene, drag, size);
    ViewRect {
        x: rect.x + (rect.width - icon_size) / 2.0,
        y: drag.current.y - icon_size / 2.0,
        width: icon_size,
        height: icon_size,
    }
}

fn drag_preview_icon_size(
    scene: &ShellScene,
    drag: &ShellInternalDrag,
    size: PhysicalSize<u32>,
) -> f32 {
    match &drag.source {
        ShellInternalDragSource::PaneItem { pane, index, .. } => {
            let view = scene.pane_view(*pane);
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
            view.map(|view| scene.drag_preview_icon_size_for_pane_item(view, item_layout))
                .unwrap_or_else(|| scene.scale_metric(128.0))
        }
        ShellInternalDragSource::Place { .. } => scene.scale_metric(PLACES_ICON_SIZE),
    }
}

fn drag_preview_icon_outline(scene: &ShellScene) -> f32 {
    scene.scale_metric(DND_PREVIEW_ICON_OUTLINE)
}

fn push_deepin_drag_icon_stack(
    paint: &DragPreviewPaintContext<'_>,
    vertices: &mut Vec<QuadVertex>,
    icons: &mut IconFrameBuilder<'_>,
    top_icon: ViewRect,
    drag: &ShellInternalDrag,
) {
    let ghost_offsets = [
        (
            paint.scene.scale_metric(-4.0),
            paint.scene.scale_metric(6.0),
            0.50,
        ),
        (
            paint.scene.scale_metric(7.0),
            paint.scene.scale_metric(-5.0),
            0.40,
        ),
        (
            paint.scene.scale_metric(-9.0),
            paint.scene.scale_metric(-4.0),
            0.30,
        ),
    ];
    let ghost_count = drag.paths.len().saturating_sub(1).min(ghost_offsets.len());
    let radius = (top_icon.width.min(top_icon.height) * 0.08).max(1.0);
    for &(dx, dy, opacity) in ghost_offsets.iter().take(ghost_count).rev() {
        let icon = translated_rect(top_icon, dx, dy);
        push_drag_preview_icon_shadow(paint, vertices, icon, radius);
        push_drag_preview_icon(paint, vertices, icons, icon, drag);
        push_clipped_rounded_rect(
            vertices,
            icon,
            paint.clip,
            radius,
            [0.0, 0.0, 0.0, (1.0 - opacity) * 0.42],
            paint.size,
        );
    }
    push_drag_preview_icon_shadow(paint, vertices, top_icon, radius);
    push_drag_preview_icon(paint, vertices, icons, top_icon, drag);
    push_clipped_rounded_rect(
        vertices,
        top_icon,
        paint.clip,
        radius,
        [0.0, 0.0, 0.0, 0.06],
        paint.size,
    );
}

fn push_drag_preview_icon_shadow(
    paint: &DragPreviewPaintContext<'_>,
    vertices: &mut Vec<QuadVertex>,
    icon: ViewRect,
    radius: f32,
) {
    push_clipped_rounded_rect(
        vertices,
        translated_rect(icon, 0.0, paint.scene.scale_metric(2.0)),
        paint.clip,
        radius + paint.scene.scale_metric(1.0),
        [0.0, 0.0, 0.0, 0.18],
        paint.size,
    );
}

fn push_deepin_drag_count_badge(
    paint: &DragPreviewPaintContext<'_>,
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    icon: ViewRect,
    count: usize,
    badge_color: UiColor,
) {
    let badge_size = paint
        .scene
        .scale_metric(if count > 99 { 28.0 } else { 24.0 });
    let badge = ViewRect {
        x: icon.right() - badge_size / 2.0 - paint.scene.scale_metric(10.0),
        y: icon.bottom() - badge_size / 2.0 - paint.scene.scale_metric(10.0),
        width: badge_size,
        height: badge_size,
    };
    push_clipped_rounded_rect(
        vertices,
        badge,
        paint.clip,
        badge_size / 2.0,
        badge_color,
        paint.size,
    );
    let label = if count > 99 {
        "99+".to_string()
    } else {
        count.to_string()
    };
    text.push_label_aligned(
        &label,
        badge,
        paint.clip,
        TextColor::rgb(255, 255, 255),
        LabelAlignment::Center,
    );
}

fn push_deepin_drag_label(
    paint: &DragPreviewPaintContext<'_>,
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    rect: ViewRect,
    drag: &ShellInternalDrag,
) {
    if drag.label.is_empty() {
        return;
    }
    let label_rect = ViewRect {
        x: rect.x,
        y: rect.bottom() - paint.scene.scale_metric(DND_PREVIEW_LABEL_HEIGHT),
        width: rect.width,
        height: paint.scene.scale_metric(DND_PREVIEW_LABEL_HEIGHT),
    };
    let mut background = paint.theme.accent();
    background[3] = 0.90;
    push_clipped_rounded_rect(
        vertices,
        label_rect,
        paint.clip,
        paint.scene.scale_metric(4.0),
        background,
        paint.size,
    );
    text.push_label_aligned(
        &drag.label,
        label_rect,
        label_rect,
        TextColor::rgb(255, 255, 255),
        LabelAlignment::Center,
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
