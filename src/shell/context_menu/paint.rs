use cosmic_text::Color as TextColor;
use fika_core::ViewRect;
use winit::dpi::PhysicalSize;

use crate::shell::context_menu::{
    ShellContextMenu, ShellContextMenuAction, ShellContextMenuCommand, ShellContextMenuIcon,
    ShellContextMenuItem, context_menu_items, context_menu_separator_before,
    context_submenu_actions,
};
use crate::shell::drop_menu::{
    ShellDropMenu, ShellDropMenuCommand, ShellDropMenuIcon, drop_menu_items,
};
use crate::shell::icon_roles::NamedIconFallback;
use crate::shell::menu_geometry::{
    context_menu_rect_scaled, context_menu_submenu_rect, drop_menu_rect_scaled,
    scaled_context_menu_metric,
};
use crate::shell::metrics::{
    CONTEXT_MENU_ICON_SIZE, CONTEXT_MENU_ROW_HEIGHT, CONTEXT_MENU_TEXT_LINE_HEIGHT,
    CONTEXT_MENU_VERTICAL_PADDING,
};
use crate::{
    IconDrawLayer, IconFrameBuilder, LabelAlignment, QuadVertex, TextFrameBuilder,
    push_clipped_rect, push_clipped_rect_outline, push_clipped_rounded_rect, push_rect,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContextMenuGlyph {
    Open,
    OpenWith,
    Pane,
    Hidden,
    Copy,
    Cut,
    Location,
    Rename,
    Trash,
    Restore,
    Delete,
    Place,
    Create,
    Paste,
    Select,
    Refresh,
    Properties,
    Remove,
}

pub(crate) fn push_drop_menu_overlay(
    menu: &ShellDropMenu,
    scale: f32,
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    size: PhysicalSize<u32>,
) {
    let rect = drop_menu_rect_scaled(menu, size, scale);
    let padding_y = scaled_context_menu_metric(CONTEXT_MENU_VERTICAL_PADDING, scale);
    let row_height = scaled_context_menu_metric(CONTEXT_MENU_ROW_HEIGHT, scale);
    let row_padding_x = scaled_context_menu_metric(8.0, scale);
    let gap = scaled_context_menu_metric(8.0, scale);
    let icon_size = scaled_context_menu_metric(CONTEXT_MENU_ICON_SIZE, scale);
    let text_height = scaled_context_menu_metric(CONTEXT_MENU_TEXT_LINE_HEIGHT, scale);
    let clip = screen_rect(size);
    push_context_menu_shadow(vertices, rect, clip, scale, size);
    push_clipped_rounded_rect(
        vertices,
        rect,
        clip,
        scaled_context_menu_metric(6.0, scale),
        [1.000, 1.000, 1.000, 1.0],
        size,
    );

    for (row, item) in drop_menu_items().iter().enumerate() {
        let row_rect = ViewRect {
            x: rect.x,
            y: rect.y + padding_y + row as f32 * row_height,
            width: rect.width,
            height: row_height,
        };
        if menu.hovered_row == Some(row) {
            push_rect(vertices, row_rect, [0.918, 0.945, 1.000, 1.0], size);
        }
        if matches!(item.command, ShellDropMenuCommand::Cancel) {
            push_clipped_rect(
                vertices,
                ViewRect {
                    x: rect.x + row_padding_x,
                    y: row_rect.y,
                    width: (rect.width - row_padding_x * 2.0).max(1.0),
                    height: scale.round().max(1.0),
                },
                rect,
                [0.898, 0.906, 0.922, 1.0],
                size,
            );
        }
        let icon = ViewRect {
            x: row_rect.x + row_padding_x,
            y: row_rect.y + (row_rect.height - icon_size) / 2.0,
            width: icon_size,
            height: icon_size,
        };
        let (glyph, fg, bg) = drop_menu_item_icon_style(item.icon);
        push_context_menu_icon(vertices, icon, rect, glyph, fg, bg, scale, size);
        let text_x = icon.right() + gap;
        text.push_label_aligned(
            item.label,
            ViewRect {
                x: text_x,
                y: row_rect.y + (row_rect.height - text_height) / 2.0,
                width: (row_rect.right() - text_x - row_padding_x).max(1.0),
                height: text_height,
            },
            rect,
            menu_text_color(menu.hovered_row == Some(row)),
            LabelAlignment::Start,
        );
    }
    push_clipped_rect_outline(vertices, rect, clip, 1.0, [0.784, 0.808, 0.839, 1.0], size);
}

pub(crate) fn push_context_menu_overlay(
    menu: &ShellContextMenu,
    show_hidden: bool,
    scale: f32,
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    icons: &mut IconFrameBuilder<'_>,
    size: PhysicalSize<u32>,
) {
    let rect = context_menu_rect_scaled(menu, size, scale);
    let padding_y = scaled_context_menu_metric(CONTEXT_MENU_VERTICAL_PADDING, scale);
    let row_height = scaled_context_menu_metric(CONTEXT_MENU_ROW_HEIGHT, scale);
    let row_padding_x = scaled_context_menu_metric(8.0, scale);
    let gap = scaled_context_menu_metric(8.0, scale);
    let icon_size = scaled_context_menu_metric(CONTEXT_MENU_ICON_SIZE, scale);
    let text_height = scaled_context_menu_metric(CONTEXT_MENU_TEXT_LINE_HEIGHT, scale);
    let clip = screen_rect(size);
    push_context_menu_shadow(vertices, rect, clip, scale, size);
    push_clipped_rounded_rect(
        vertices,
        rect,
        clip,
        scaled_context_menu_metric(6.0, scale),
        [1.000, 1.000, 1.000, 1.0],
        size,
    );

    let items = context_menu_items(menu);
    for (row, item) in items.iter().enumerate() {
        let row_rect = ViewRect {
            x: rect.x,
            y: rect.y + padding_y + row as f32 * row_height,
            width: rect.width,
            height: row_height,
        };
        if menu.hovered_row == Some(row) {
            push_rect(vertices, row_rect, [0.918, 0.945, 1.000, 1.0], size);
        }
        if context_menu_separator_before(&menu.target, row) {
            push_clipped_rect(
                vertices,
                ViewRect {
                    x: rect.x + row_padding_x,
                    y: row_rect.y,
                    width: (rect.width - row_padding_x * 2.0).max(1.0),
                    height: scale.round().max(1.0),
                },
                rect,
                [0.898, 0.906, 0.922, 1.0],
                size,
            );
        }
        let icon = ViewRect {
            x: row_rect.x + row_padding_x,
            y: row_rect.y + (row_rect.height - icon_size) / 2.0,
            width: icon_size,
            height: icon_size,
        };
        push_context_menu_item_icon(vertices, icons, item, icon, rect, scale, size);
        let text_x = icon.right() + gap;
        text.push_label_aligned(
            context_menu_item_label(item, show_hidden).as_str(),
            ViewRect {
                x: text_x,
                y: row_rect.y + (row_rect.height - text_height) / 2.0,
                width: (row_rect.right()
                    - text_x
                    - row_padding_x
                    - if item.submenu.is_some() { gap } else { 0.0 })
                .max(1.0),
                height: text_height,
            },
            rect,
            menu_text_color(menu.hovered_row == Some(row)),
            LabelAlignment::Start,
        );
        if item.submenu.is_some() {
            text.push_label_aligned(
                ">",
                ViewRect {
                    x: row_rect.right() - row_padding_x - gap,
                    y: row_rect.y + (row_rect.height - text_height) / 2.0,
                    width: gap,
                    height: text_height,
                },
                rect,
                TextColor::rgb(89, 99, 110),
                LabelAlignment::Center,
            );
        }
    }
    push_clipped_rect_outline(vertices, rect, clip, 1.0, [0.784, 0.808, 0.839, 1.0], size);
    push_context_submenu_overlay(menu, show_hidden, scale, vertices, text, icons, size);
}

pub(crate) fn context_menu_named_icon_request(
    item: &ShellContextMenuItem,
) -> Option<(&str, NamedIconFallback)> {
    match &item.icon {
        ShellContextMenuIcon::Service(Some(icon)) => {
            let icon = icon.trim();
            (!icon.is_empty()).then_some((icon, NamedIconFallback::Service))
        }
        ShellContextMenuIcon::Service(None) => Some(("system-run", NamedIconFallback::Service)),
        ShellContextMenuIcon::Application(Some(icon)) => {
            let icon = icon.trim();
            (!icon.is_empty()).then_some((icon, NamedIconFallback::Application))
        }
        ShellContextMenuIcon::Application(None) => {
            Some(("application-x-executable", NamedIconFallback::Application))
        }
        _ => None,
    }
}

fn push_context_submenu_overlay(
    menu: &ShellContextMenu,
    show_hidden: bool,
    scale: f32,
    vertices: &mut Vec<QuadVertex>,
    text: &mut TextFrameBuilder<'_>,
    icons: &mut IconFrameBuilder<'_>,
    size: PhysicalSize<u32>,
) {
    let Some(submenu) = menu.active_submenu else {
        return;
    };
    let Some(rect) = context_menu_submenu_rect(menu, size, scale) else {
        return;
    };
    let items = context_submenu_actions(submenu, menu);
    let padding_y = scaled_context_menu_metric(CONTEXT_MENU_VERTICAL_PADDING, scale);
    let row_height = scaled_context_menu_metric(CONTEXT_MENU_ROW_HEIGHT, scale);
    let row_padding_x = scaled_context_menu_metric(8.0, scale);
    let gap = scaled_context_menu_metric(8.0, scale);
    let icon_size = scaled_context_menu_metric(CONTEXT_MENU_ICON_SIZE, scale);
    let text_height = scaled_context_menu_metric(CONTEXT_MENU_TEXT_LINE_HEIGHT, scale);
    let clip = screen_rect(size);
    push_context_menu_shadow(vertices, rect, clip, scale, size);
    push_clipped_rounded_rect(
        vertices,
        rect,
        clip,
        scaled_context_menu_metric(6.0, scale),
        [1.000, 1.000, 1.000, 1.0],
        size,
    );
    for (row, item) in items.iter().enumerate() {
        let row_rect = ViewRect {
            x: rect.x,
            y: rect.y + padding_y + row as f32 * row_height,
            width: rect.width,
            height: row_height,
        };
        if menu.hovered_submenu_row == Some(row) {
            push_rect(vertices, row_rect, [0.918, 0.945, 1.000, 1.0], size);
        }
        if item.separator_before {
            push_clipped_rect(
                vertices,
                ViewRect {
                    x: rect.x + row_padding_x,
                    y: row_rect.y,
                    width: (rect.width - row_padding_x * 2.0).max(1.0),
                    height: scale.round().max(1.0),
                },
                rect,
                [0.898, 0.906, 0.922, 1.0],
                size,
            );
        }
        let icon = ViewRect {
            x: row_rect.x + row_padding_x,
            y: row_rect.y + (row_rect.height - icon_size) / 2.0,
            width: icon_size,
            height: icon_size,
        };
        push_context_menu_item_icon(vertices, icons, item, icon, rect, scale, size);
        let text_x = icon.right() + gap;
        text.push_label_aligned(
            context_menu_item_label(item, show_hidden).as_str(),
            ViewRect {
                x: text_x,
                y: row_rect.y + (row_rect.height - text_height) / 2.0,
                width: (row_rect.right() - text_x - row_padding_x).max(1.0),
                height: text_height,
            },
            rect,
            menu_text_color(menu.hovered_submenu_row == Some(row)),
            LabelAlignment::Start,
        );
    }
    push_clipped_rect_outline(vertices, rect, clip, 1.0, [0.784, 0.808, 0.839, 1.0], size);
}

fn push_context_menu_item_icon(
    vertices: &mut Vec<QuadVertex>,
    icons: &mut IconFrameBuilder<'_>,
    item: &ShellContextMenuItem,
    icon: ViewRect,
    clip: ViewRect,
    scale: f32,
    size: PhysicalSize<u32>,
) {
    if let Some((icon_name, fallback)) = context_menu_named_icon_request(item)
        && icons.push_named_theme_icon(icon_name, fallback, icon, clip, IconDrawLayer::Overlay)
    {
        return;
    }
    let (glyph, icon_fg, icon_bg) = context_menu_item_icon_style(item);
    push_context_menu_icon(vertices, icon, clip, glyph, icon_fg, icon_bg, scale, size);
}

fn context_menu_icon_style(
    action: ShellContextMenuAction,
) -> (ContextMenuGlyph, [f32; 4], [f32; 4]) {
    match action {
        ShellContextMenuAction::OpenWith => (
            ContextMenuGlyph::OpenWith,
            [0.263, 0.220, 0.792, 1.0],
            [0.933, 0.929, 1.000, 1.0],
        ),
        ShellContextMenuAction::OpenInNewPane => (
            ContextMenuGlyph::Pane,
            [0.114, 0.306, 0.847, 1.0],
            [0.918, 0.945, 1.000, 1.0],
        ),
        ShellContextMenuAction::SplitPane => (
            ContextMenuGlyph::Pane,
            [0.114, 0.306, 0.847, 1.0],
            [0.918, 0.945, 1.000, 1.0],
        ),
        ShellContextMenuAction::Copy => (
            ContextMenuGlyph::Copy,
            [0.145, 0.388, 0.922, 1.0],
            [0.918, 0.945, 1.000, 1.0],
        ),
        ShellContextMenuAction::Cut => (
            ContextMenuGlyph::Cut,
            [0.706, 0.325, 0.035, 1.0],
            [1.000, 0.953, 0.875, 1.0],
        ),
        ShellContextMenuAction::CopyLocation => (
            ContextMenuGlyph::Location,
            [0.200, 0.255, 0.333, 1.0],
            [0.910, 0.933, 0.969, 1.0],
        ),
        ShellContextMenuAction::Rename => (
            ContextMenuGlyph::Rename,
            [0.427, 0.157, 0.851, 1.0],
            [0.949, 0.929, 1.000, 1.0],
        ),
        ShellContextMenuAction::RenameAsAdministrator => (
            ContextMenuGlyph::Rename,
            [0.706, 0.325, 0.035, 1.0],
            [1.000, 0.953, 0.875, 1.0],
        ),
        ShellContextMenuAction::MoveToTrash | ShellContextMenuAction::EmptyTrash => (
            ContextMenuGlyph::Trash,
            [0.725, 0.110, 0.110, 1.0],
            [1.000, 0.910, 0.910, 1.0],
        ),
        ShellContextMenuAction::MoveToTrashAsAdministrator => (
            ContextMenuGlyph::Trash,
            [0.706, 0.325, 0.035, 1.0],
            [1.000, 0.953, 0.875, 1.0],
        ),
        ShellContextMenuAction::RestoreFromTrash => (
            ContextMenuGlyph::Restore,
            [0.016, 0.471, 0.341, 1.0],
            [0.906, 0.973, 0.937, 1.0],
        ),
        ShellContextMenuAction::DeletePermanently => (
            ContextMenuGlyph::Delete,
            [0.725, 0.110, 0.110, 1.0],
            [1.000, 0.910, 0.910, 1.0],
        ),
        ShellContextMenuAction::AddToPlaces => (
            ContextMenuGlyph::Place,
            [0.059, 0.463, 0.431, 1.0],
            [0.902, 1.000, 0.984, 1.0],
        ),
        ShellContextMenuAction::AddNetworkFolder => (
            ContextMenuGlyph::Place,
            [0.059, 0.463, 0.431, 1.0],
            [0.902, 1.000, 0.984, 1.0],
        ),
        ShellContextMenuAction::CreateNew => (
            ContextMenuGlyph::Create,
            [0.059, 0.298, 0.506, 1.0],
            [0.906, 0.945, 0.984, 1.0],
        ),
        ShellContextMenuAction::Paste => (
            ContextMenuGlyph::Paste,
            [0.016, 0.471, 0.341, 1.0],
            [0.906, 0.973, 0.937, 1.0],
        ),
        ShellContextMenuAction::PasteAsAdministrator => (
            ContextMenuGlyph::Paste,
            [0.706, 0.325, 0.035, 1.0],
            [1.000, 0.953, 0.875, 1.0],
        ),
        ShellContextMenuAction::SelectAll => (
            ContextMenuGlyph::Select,
            [0.122, 0.310, 0.749, 1.0],
            [0.918, 0.945, 1.000, 1.0],
        ),
        ShellContextMenuAction::ViewMode => (
            ContextMenuGlyph::Pane,
            [0.114, 0.306, 0.847, 1.0],
            [0.918, 0.945, 1.000, 1.0],
        ),
        ShellContextMenuAction::ToggleHiddenFiles => (
            ContextMenuGlyph::Hidden,
            [0.294, 0.318, 0.357, 1.0],
            [0.933, 0.945, 0.961, 1.0],
        ),
        ShellContextMenuAction::Refresh => (
            ContextMenuGlyph::Refresh,
            [0.059, 0.463, 0.431, 1.0],
            [0.902, 1.000, 0.984, 1.0],
        ),
        ShellContextMenuAction::Properties => (
            ContextMenuGlyph::Properties,
            [0.216, 0.255, 0.318, 1.0],
            [0.933, 0.945, 0.961, 1.0],
        ),
        ShellContextMenuAction::RemovePlace => (
            ContextMenuGlyph::Remove,
            [0.725, 0.110, 0.110, 1.0],
            [1.000, 0.910, 0.910, 1.0],
        ),
        ShellContextMenuAction::MountDevice => (
            ContextMenuGlyph::Restore,
            [0.016, 0.471, 0.341, 1.0],
            [0.906, 0.973, 0.937, 1.0],
        ),
        ShellContextMenuAction::UnmountDevice
        | ShellContextMenuAction::EjectDevice
        | ShellContextMenuAction::SafelyRemoveDevice => (
            ContextMenuGlyph::Open,
            [0.706, 0.325, 0.035, 1.0],
            [1.000, 0.953, 0.875, 1.0],
        ),
    }
}

fn context_menu_item_label(item: &ShellContextMenuItem, show_hidden: bool) -> String {
    match item.command {
        ShellContextMenuCommand::Builtin(action) => {
            action.label_for_hidden_state(show_hidden).to_string()
        }
        _ => item.label.clone(),
    }
}

fn context_menu_item_icon_style(
    item: &ShellContextMenuItem,
) -> (ContextMenuGlyph, [f32; 4], [f32; 4]) {
    match &item.icon {
        ShellContextMenuIcon::Builtin(action) => context_menu_icon_style(*action),
        ShellContextMenuIcon::Service(_) => (
            ContextMenuGlyph::Properties,
            [0.216, 0.255, 0.318, 1.0],
            [0.933, 0.945, 0.961, 1.0],
        ),
        ShellContextMenuIcon::Application(_) => (
            ContextMenuGlyph::OpenWith,
            [0.263, 0.220, 0.792, 1.0],
            [0.933, 0.929, 1.000, 1.0],
        ),
    }
}

fn drop_menu_item_icon_style(icon: ShellDropMenuIcon) -> (ContextMenuGlyph, [f32; 4], [f32; 4]) {
    match icon {
        ShellDropMenuIcon::Copy => context_menu_icon_style(ShellContextMenuAction::Copy),
        ShellDropMenuIcon::Move => context_menu_icon_style(ShellContextMenuAction::Cut),
        ShellDropMenuIcon::Link => context_menu_icon_style(ShellContextMenuAction::CopyLocation),
        ShellDropMenuIcon::Cancel => context_menu_icon_style(ShellContextMenuAction::RemovePlace),
    }
}

pub(crate) fn push_context_menu_shadow(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    clip: ViewRect,
    scale_factor: f32,
    size: PhysicalSize<u32>,
) {
    let scale = scale_factor.max(1.0);
    let radius = (6.0 * scale).round().max(1.0);
    for (dy, spread, alpha) in [(1.0, 1.0, 0.10), (3.0, 3.0, 0.08), (7.0, 8.0, 0.05)] {
        push_clipped_rounded_rect(
            vertices,
            ViewRect {
                x: rect.x - (spread * scale).round(),
                y: rect.y + (dy * scale).round() - (spread * scale).round(),
                width: rect.width + (spread * 2.0 * scale).round(),
                height: rect.height + (spread * 2.0 * scale).round(),
            },
            clip,
            radius + (spread * scale).round(),
            [0.000, 0.000, 0.000, alpha],
            size,
        );
    }
}

fn push_context_menu_icon(
    vertices: &mut Vec<QuadVertex>,
    rect: ViewRect,
    clip: ViewRect,
    glyph: ContextMenuGlyph,
    fg: [f32; 4],
    bg: [f32; 4],
    scale_factor: f32,
    size: PhysicalSize<u32>,
) {
    push_clipped_rounded_rect(
        vertices,
        rect,
        clip,
        (5.0 * scale_factor).round().max(1.0),
        bg,
        size,
    );
    match glyph {
        ContextMenuGlyph::Open => {
            push_context_icon_piece(vertices, rect, clip, 5.0, 5.0, 6.0, 3.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 4.0, 7.0, 10.0, 7.0, 2.0, fg, size);
        }
        ContextMenuGlyph::OpenWith => {
            for (x, y) in [(5.0, 5.0), (10.0, 5.0), (5.0, 10.0), (10.0, 10.0)] {
                push_context_icon_piece(vertices, rect, clip, x, y, 3.0, 3.0, 1.0, fg, size);
            }
        }
        ContextMenuGlyph::Pane => {
            push_context_icon_piece(vertices, rect, clip, 4.0, 4.0, 10.0, 10.0, 2.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 8.0, 5.0, 1.0, 8.0, 0.0, bg, size);
            push_context_icon_piece(vertices, rect, clip, 5.0, 8.0, 8.0, 1.0, 0.0, bg, size);
        }
        ContextMenuGlyph::Hidden => {
            push_context_icon_piece(vertices, rect, clip, 4.0, 8.0, 10.0, 3.0, 2.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 7.0, 6.0, 4.0, 7.0, 2.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 8.0, 8.0, 2.0, 3.0, 1.0, bg, size);
        }
        ContextMenuGlyph::Copy => {
            push_context_icon_piece(vertices, rect, clip, 6.0, 4.0, 7.0, 9.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 4.0, 6.0, 7.0, 9.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 5.0, 7.0, 5.0, 7.0, 0.0, bg, size);
        }
        ContextMenuGlyph::Cut => {
            push_context_icon_piece(vertices, rect, clip, 4.0, 5.0, 3.0, 3.0, 2.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 4.0, 11.0, 3.0, 3.0, 2.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 8.0, 6.0, 6.0, 2.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 8.0, 11.0, 6.0, 2.0, 1.0, fg, size);
        }
        ContextMenuGlyph::Location => {
            push_context_icon_piece(vertices, rect, clip, 5.0, 4.0, 8.0, 8.0, 4.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 8.0, 7.0, 2.0, 2.0, 1.0, bg, size);
            push_context_icon_piece(vertices, rect, clip, 8.0, 11.0, 2.0, 4.0, 1.0, fg, size);
        }
        ContextMenuGlyph::Rename => {
            push_context_icon_piece(vertices, rect, clip, 4.0, 10.0, 8.0, 3.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 11.0, 8.0, 3.0, 3.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 4.0, 14.0, 9.0, 1.0, 0.0, fg, size);
        }
        ContextMenuGlyph::Trash => {
            push_context_icon_piece(vertices, rect, clip, 5.0, 5.0, 8.0, 2.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 6.0, 8.0, 6.0, 7.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 7.0, 9.0, 1.0, 5.0, 0.0, bg, size);
            push_context_icon_piece(vertices, rect, clip, 10.0, 9.0, 1.0, 5.0, 0.0, bg, size);
        }
        ContextMenuGlyph::Restore => {
            push_context_icon_piece(vertices, rect, clip, 5.0, 5.0, 2.0, 8.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 6.0, 11.0, 7.0, 2.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 11.0, 8.0, 2.0, 4.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 9.0, 7.0, 5.0, 2.0, 1.0, fg, size);
        }
        ContextMenuGlyph::Delete => {
            push_context_icon_piece(vertices, rect, clip, 5.0, 5.0, 2.0, 2.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 8.0, 8.0, 2.0, 2.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 11.0, 11.0, 2.0, 2.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 11.0, 5.0, 2.0, 2.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 5.0, 11.0, 2.0, 2.0, 1.0, fg, size);
        }
        ContextMenuGlyph::Place => {
            push_context_icon_piece(vertices, rect, clip, 5.0, 4.0, 8.0, 11.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 7.0, 11.0, 4.0, 4.0, 0.0, bg, size);
        }
        ContextMenuGlyph::Create => {
            push_context_icon_piece(vertices, rect, clip, 8.0, 4.0, 2.0, 10.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 4.0, 8.0, 10.0, 2.0, 1.0, fg, size);
        }
        ContextMenuGlyph::Paste => {
            push_context_icon_piece(vertices, rect, clip, 5.0, 5.0, 8.0, 10.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 7.0, 4.0, 4.0, 3.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 7.0, 9.0, 4.0, 1.0, 0.0, bg, size);
            push_context_icon_piece(vertices, rect, clip, 7.0, 12.0, 4.0, 1.0, 0.0, bg, size);
        }
        ContextMenuGlyph::Select => {
            push_context_icon_piece(vertices, rect, clip, 5.0, 5.0, 8.0, 2.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 5.0, 11.0, 8.0, 2.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 5.0, 5.0, 2.0, 8.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 11.0, 5.0, 2.0, 8.0, 1.0, fg, size);
        }
        ContextMenuGlyph::Refresh => {
            push_context_icon_piece(vertices, rect, clip, 5.0, 5.0, 8.0, 2.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 5.0, 5.0, 2.0, 8.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 5.0, 11.0, 8.0, 2.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 11.0, 9.0, 2.0, 4.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 10.0, 4.0, 4.0, 4.0, 1.0, fg, size);
        }
        ContextMenuGlyph::Properties => {
            push_context_icon_piece(vertices, rect, clip, 8.0, 4.0, 2.0, 2.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 8.0, 8.0, 2.0, 6.0, 1.0, fg, size);
            push_context_icon_piece(vertices, rect, clip, 7.0, 14.0, 4.0, 1.0, 0.0, fg, size);
        }
        ContextMenuGlyph::Remove => {
            push_context_icon_piece(vertices, rect, clip, 4.0, 8.0, 10.0, 2.0, 1.0, fg, size);
        }
    }
}

fn push_context_icon_piece(
    vertices: &mut Vec<QuadVertex>,
    bounds: ViewRect,
    clip: ViewRect,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radius: f32,
    color: [f32; 4],
    size: PhysicalSize<u32>,
) {
    let unit = bounds.width.min(bounds.height) / CONTEXT_MENU_ICON_SIZE;
    let piece = ViewRect {
        x: bounds.x + (x * unit).round(),
        y: bounds.y + (y * unit).round(),
        width: (width * unit).round().max(1.0),
        height: (height * unit).round().max(1.0),
    };
    push_clipped_rounded_rect(vertices, piece, clip, (radius * unit).round(), color, size);
}

fn screen_rect(size: PhysicalSize<u32>) -> ViewRect {
    ViewRect {
        x: 0.0,
        y: 0.0,
        width: size.width.max(1) as f32,
        height: size.height.max(1) as f32,
    }
}

fn menu_text_color(hovered: bool) -> TextColor {
    if hovered {
        TextColor::rgb(31, 79, 191)
    } else {
        TextColor::rgb(36, 41, 47)
    }
}
