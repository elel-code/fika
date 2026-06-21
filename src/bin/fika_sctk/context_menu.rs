use std::path::PathBuf;

use fika_core::{ViewPoint, ViewRect};

use super::metrics::{TEXT_FONT_SIZE, TEXT_LINE_HEIGHT};
use super::quad::QuadBatch;
use super::text::TextBatch;

const MENU_WIDTH: f32 = 196.0;
const ROW_HEIGHT: f32 = 28.0;
const VERTICAL_PADDING: f32 = 4.0;
const VIEWPORT_MARGIN: f32 = 8.0;
const TEXT_PRIMARY: [u8; 4] = [36, 41, 47, 255];
const TEXT_MUTED: [u8; 4] = [110, 118, 128, 255];

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SctkContextMenu {
    pub(crate) target: ContextTarget,
    pub(crate) position: ViewPoint,
    pub(crate) hover: Option<usize>,
}

impl SctkContextMenu {
    pub(crate) fn new(target: ContextTarget, position: ViewPoint) -> Self {
        Self {
            target,
            position,
            hover: None,
        }
    }

    pub(crate) fn actions(&self, show_hidden: bool, split_enabled: bool) -> Vec<ContextActionRow> {
        context_actions(&self.target, show_hidden, split_enabled)
    }

    pub(crate) fn set_pointer(&mut self, point: ViewPoint, width: u32, height: u32) -> bool {
        let before = self.hover;
        self.hover = row_at_position(
            point,
            self.position,
            self.actions(false, false).len(),
            width,
            height,
        );
        before != self.hover
    }

    pub(crate) fn action_at(
        &self,
        point: ViewPoint,
        show_hidden: bool,
        split_enabled: bool,
        width: u32,
        height: u32,
    ) -> Option<ContextAction> {
        let actions = self.actions(show_hidden, split_enabled);
        let row = row_at_position(point, self.position, actions.len(), width, height)?;
        actions
            .get(row)
            .filter(|action| action.enabled)
            .map(|action| action.action)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ContextTarget {
    Item {
        pane: ContextPane,
        path: PathBuf,
        is_dir: bool,
        selection_count: usize,
    },
    Blank {
        pane: ContextPane,
        path: PathBuf,
    },
    Place {
        label: String,
        path: PathBuf,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ContextPane(pub(crate) u8);

impl ContextPane {
    pub(crate) const PRIMARY: Self = Self(1);
    pub(crate) const SPLIT: Self = Self(2);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContextAction {
    Open,
    OpenInSplit,
    CopyLocation,
    MoveToTrash,
    Rename,
    CreateFolder,
    CreateFile,
    ToggleHidden,
    SplitView,
    SelectAll,
    Refresh,
    Properties,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ContextActionRow {
    pub(crate) action: ContextAction,
    pub(crate) label: String,
    pub(crate) enabled: bool,
    pub(crate) separator_before: bool,
}

pub(crate) fn push_context_menu(
    menu: &SctkContextMenu,
    batch: &mut QuadBatch,
    text: &mut TextBatch,
    show_hidden: bool,
    split_enabled: bool,
    width: u32,
    height: u32,
) {
    let actions = menu.actions(show_hidden, split_enabled);
    if actions.is_empty() {
        return;
    }
    let rect = menu_rect(menu.position, actions.len(), width, height);
    let window = ViewRect {
        x: 0.0,
        y: 0.0,
        width: width as f32,
        height: height as f32,
    };
    batch.push_clipped_rounded_rect(rect, window, 6.0, [0.985, 0.99, 0.995, 1.0], width, height);
    push_menu_border(batch, rect, window, width, height);

    for (index, row) in actions.iter().enumerate() {
        let row_rect = menu_row_rect(rect, index);
        if row.separator_before {
            batch.push_clipped_rect(
                ViewRect {
                    x: rect.x + 8.0,
                    y: row_rect.y - 1.0,
                    width: rect.width - 16.0,
                    height: 1.0,
                },
                rect,
                [0.78, 0.81, 0.85, 1.0],
                width,
                height,
            );
        }
        if menu.hover == Some(index) && row.enabled {
            batch.push_clipped_rounded_rect(
                ViewRect {
                    x: row_rect.x + 4.0,
                    y: row_rect.y + 2.0,
                    width: row_rect.width - 8.0,
                    height: row_rect.height - 4.0,
                },
                rect,
                5.0,
                [0.78, 0.86, 0.95, 1.0],
                width,
                height,
            );
        }
        push_action_glyph(
            batch,
            row.action,
            row_rect,
            rect,
            row.enabled,
            width,
            height,
        );
        text.push_no_wrap(
            row.label.as_str(),
            ViewRect {
                x: row_rect.x + 32.0,
                y: row_rect.y + (row_rect.height - TEXT_LINE_HEIGHT) / 2.0,
                width: (row_rect.width - 42.0).max(1.0),
                height: TEXT_LINE_HEIGHT,
            },
            rect,
            TEXT_FONT_SIZE,
            TEXT_LINE_HEIGHT,
            if row.enabled {
                TEXT_PRIMARY
            } else {
                TEXT_MUTED
            },
        );
    }
}

pub(crate) fn row_at_position(
    point: ViewPoint,
    anchor: ViewPoint,
    action_count: usize,
    width: u32,
    height: u32,
) -> Option<usize> {
    let rect = menu_rect(anchor, action_count, width, height);
    let x_in_rect = point.x >= rect.x && point.x < rect.x + rect.width;
    let y = point.y - rect.y - VERTICAL_PADDING;
    let row = (y / ROW_HEIGHT).floor() as isize;
    (x_in_rect && row >= 0 && (row as usize) < action_count).then_some(row as usize)
}

pub(crate) fn menu_rect(
    anchor: ViewPoint,
    action_count: usize,
    width: u32,
    height: u32,
) -> ViewRect {
    let viewport_width = width as f32;
    let viewport_height = height as f32;
    let menu_width = (viewport_width - VIEWPORT_MARGIN * 2.0)
        .max(1.0)
        .min(MENU_WIDTH);
    let menu_height = (VERTICAL_PADDING * 2.0 + action_count as f32 * ROW_HEIGHT)
        .min((viewport_height - VIEWPORT_MARGIN * 2.0).max(1.0));
    ViewRect {
        x: popup_axis(anchor.x, menu_width, viewport_width),
        y: popup_axis(anchor.y, menu_height, viewport_height),
        width: menu_width,
        height: menu_height,
    }
}

pub(crate) fn context_actions(
    target: &ContextTarget,
    show_hidden: bool,
    split_enabled: bool,
) -> Vec<ContextActionRow> {
    match target {
        ContextTarget::Item {
            is_dir,
            selection_count,
            ..
        } => {
            let label = if *selection_count > 1 {
                format!("Move {selection_count} Items to Trash")
            } else {
                "Move to Trash".to_string()
            };
            let mut rows = Vec::new();
            rows.push(row(ContextAction::Open, "Open", *is_dir, false));
            rows.push(row(
                ContextAction::OpenInSplit,
                "Open in Split Pane",
                *is_dir,
                false,
            ));
            if *is_dir {
                rows.push(row(
                    ContextAction::CreateFolder,
                    "Create Folder",
                    true,
                    false,
                ));
                rows.push(row(
                    ContextAction::CreateFile,
                    "Create Text File",
                    true,
                    false,
                ));
            }
            rows.push(row(
                ContextAction::CopyLocation,
                "Copy Location",
                true,
                true,
            ));
            rows.push(row(ContextAction::MoveToTrash, label, true, false));
            rows.push(row(ContextAction::Rename, "Rename", true, false));
            rows.push(row(ContextAction::Properties, "Properties", true, true));
            rows
        }
        ContextTarget::Blank { .. } => vec![
            row(ContextAction::CreateFolder, "Create Folder", true, false),
            row(ContextAction::CreateFile, "Create Text File", true, false),
            row(
                ContextAction::ToggleHidden,
                if show_hidden {
                    "Hide Hidden Files"
                } else {
                    "Show Hidden Files"
                },
                true,
                true,
            ),
            row(
                ContextAction::SplitView,
                if split_enabled {
                    "Close Split View"
                } else {
                    "Split View"
                },
                true,
                false,
            ),
            row(ContextAction::SelectAll, "Select All", true, true),
            row(ContextAction::Refresh, "Refresh", true, false),
            row(ContextAction::Properties, "Properties", true, true),
        ],
        ContextTarget::Place { .. } => vec![
            row(ContextAction::Open, "Open", true, false),
            row(
                ContextAction::OpenInSplit,
                "Open in Split Pane",
                true,
                false,
            ),
            row(ContextAction::CopyLocation, "Copy Location", true, true),
            row(ContextAction::Properties, "Properties", true, true),
        ],
    }
}

fn row(
    action: ContextAction,
    label: impl Into<String>,
    enabled: bool,
    separator_before: bool,
) -> ContextActionRow {
    ContextActionRow {
        action,
        label: label.into(),
        enabled,
        separator_before,
    }
}

fn menu_row_rect(rect: ViewRect, index: usize) -> ViewRect {
    ViewRect {
        x: rect.x,
        y: rect.y + VERTICAL_PADDING + index as f32 * ROW_HEIGHT,
        width: rect.width,
        height: ROW_HEIGHT,
    }
}

fn popup_axis(anchor: f32, size: f32, viewport_size: f32) -> f32 {
    let min = VIEWPORT_MARGIN.min((viewport_size - size).max(0.0));
    let max = (viewport_size - size - VIEWPORT_MARGIN).max(min);
    if anchor + size <= viewport_size - VIEWPORT_MARGIN {
        return anchor.clamp(min, max);
    }
    let flipped = anchor - size;
    if flipped >= min {
        return flipped.min(max);
    }
    anchor.clamp(min, max)
}

fn push_menu_border(
    batch: &mut QuadBatch,
    rect: ViewRect,
    clip: ViewRect,
    width: u32,
    height: u32,
) {
    let color = [0.64, 0.68, 0.74, 1.0];
    batch.push_clipped_rect(
        ViewRect {
            height: 1.0,
            ..rect
        },
        clip,
        color,
        width,
        height,
    );
    batch.push_clipped_rect(
        ViewRect {
            y: rect.bottom() - 1.0,
            height: 1.0,
            ..rect
        },
        clip,
        color,
        width,
        height,
    );
    batch.push_clipped_rect(ViewRect { width: 1.0, ..rect }, clip, color, width, height);
    batch.push_clipped_rect(
        ViewRect {
            x: rect.right() - 1.0,
            width: 1.0,
            ..rect
        },
        clip,
        color,
        width,
        height,
    );
}

fn push_action_glyph(
    batch: &mut QuadBatch,
    action: ContextAction,
    row: ViewRect,
    clip: ViewRect,
    enabled: bool,
    width: u32,
    height: u32,
) {
    let base = if enabled {
        [0.30, 0.48, 0.68, 1.0]
    } else {
        [0.62, 0.66, 0.70, 1.0]
    };
    let glyph = ViewRect {
        x: row.x + 11.0,
        y: row.y + (row.height - 14.0) / 2.0,
        width: 14.0,
        height: 14.0,
    };
    match action {
        ContextAction::MoveToTrash => {
            batch.push_clipped_rounded_rect(
                glyph,
                clip,
                3.0,
                [0.70, 0.22, 0.22, 1.0],
                width,
                height,
            );
            batch.push_clipped_rect(
                ViewRect {
                    x: glyph.x + 2.0,
                    y: glyph.y - 2.0,
                    width: glyph.width - 4.0,
                    height: 2.0,
                },
                clip,
                [0.55, 0.18, 0.18, 1.0],
                width,
                height,
            );
        }
        ContextAction::CreateFolder | ContextAction::Open | ContextAction::OpenInSplit => {
            batch.push_clipped_rounded_rect(
                ViewRect {
                    x: glyph.x,
                    y: glyph.y + 4.0,
                    width: glyph.width,
                    height: glyph.height - 4.0,
                },
                clip,
                3.0,
                base,
                width,
                height,
            );
            batch.push_clipped_rounded_rect(
                ViewRect {
                    x: glyph.x + 1.0,
                    y: glyph.y + 1.0,
                    width: glyph.width * 0.55,
                    height: 5.0,
                },
                clip,
                2.0,
                base,
                width,
                height,
            );
        }
        ContextAction::CreateFile | ContextAction::Properties => {
            batch.push_clipped_rounded_rect(glyph, clip, 2.0, base, width, height);
            batch.push_clipped_rect(
                ViewRect {
                    x: glyph.x + glyph.width - 5.0,
                    y: glyph.y,
                    width: 5.0,
                    height: 5.0,
                },
                clip,
                [0.90, 0.93, 0.96, 1.0],
                width,
                height,
            );
        }
        _ => batch.push_clipped_rounded_rect(glyph, clip, 3.0, base, width, height),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_rect_flips_inside_viewport() {
        let rect = menu_rect(ViewPoint { x: 390.0, y: 290.0 }, 4, 400, 300);
        assert!(rect.right() <= 392.0);
        assert!(rect.bottom() <= 292.0);
    }

    #[test]
    fn item_menu_labels_multi_selection_trash() {
        let actions = context_actions(
            &ContextTarget::Item {
                pane: ContextPane::PRIMARY,
                path: PathBuf::from("/tmp/a"),
                is_dir: false,
                selection_count: 3,
            },
            false,
            false,
        );
        assert!(
            actions
                .iter()
                .any(|row| row.label == "Move 3 Items to Trash")
        );
    }
}
