use gpui::{InteractiveElement, IntoElement, Styled, div, px, rgb, rgba};

use crate::ui::drag_drop::FileTransferMode;

pub(super) fn place_row_background(
    active: bool,
    drop_target: Option<FileTransferMode>,
) -> gpui::Rgba {
    if let Some(mode) = drop_target {
        place_drop_target_background(mode)
    } else if active {
        rgb(0xeaf1ff)
    } else {
        rgb(0xf8f9fb)
    }
}

pub(super) fn place_row_border_color(
    active: bool,
    drop_target: Option<FileTransferMode>,
) -> gpui::Rgba {
    if let Some(mode) = drop_target {
        place_drop_target_border_color(mode)
    } else if active {
        rgb(0xbfdbfe)
    } else {
        rgba(0x00000000)
    }
}

pub(super) fn place_row_hover_background(
    active: bool,
    drop_target: Option<FileTransferMode>,
) -> gpui::Rgba {
    if let Some(mode) = drop_target {
        place_drop_target_hover_background(mode)
    } else if active {
        rgb(0xeaf1ff)
    } else {
        rgb(0xeef3f8)
    }
}

fn place_drop_target_background(mode: FileTransferMode) -> gpui::Rgba {
    match mode {
        FileTransferMode::Copy => rgba(0x16a34a34),
        FileTransferMode::Move => rgba(0xd9770634),
        FileTransferMode::Link => rgba(0x7c3aed34),
    }
}

fn place_drop_target_hover_background(mode: FileTransferMode) -> gpui::Rgba {
    match mode {
        FileTransferMode::Copy => rgba(0x16a34a4a),
        FileTransferMode::Move => rgba(0xd977064a),
        FileTransferMode::Link => rgba(0x7c3aed4a),
    }
}

fn place_drop_target_border_color(mode: FileTransferMode) -> gpui::Rgba {
    match mode {
        FileTransferMode::Copy => rgb(0x16a34a),
        FileTransferMode::Move => rgb(0xd97706),
        FileTransferMode::Link => rgb(0x7c3aed),
    }
}

pub(super) fn place_insert_indicator(id: String) -> impl IntoElement {
    div()
        .id(id)
        .mx_2()
        .h(px(2.0))
        .rounded_full()
        .bg(rgb(0x2f6fed))
}
