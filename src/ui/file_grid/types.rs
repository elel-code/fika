use fika_core::{CompactLayout, IconsLayout, PaneId, ViewRect};
use gpui::ScrollHandle;

use super::details::{DetailsItemSnapshot, DetailsLayoutMetrics};
use super::paint_slots::{DetailsPaintSnapshot, ItemPaintSnapshot};
use super::snapshot::VisibleItemSnapshot;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FileGridMode {
    Manager,
    Chooser { directories: bool, multiple: bool },
}

pub(crate) struct FileGridProps {
    pub(crate) pane_id: PaneId,
    pub(crate) snapshot: FileGridRenderSnapshot,
    pub(crate) trash_view: bool,
    pub(crate) scroll_handle: ScrollHandle,
    pub(crate) rubber_band: Option<ViewRect>,
    pub(crate) drop_target: bool,
    pub(crate) mode: FileGridMode,
}

#[derive(Clone, Debug)]
pub(crate) enum FileGridSnapshot {
    Compact {
        layout: CompactLayout,
        items: Vec<VisibleItemSnapshot>,
    },
    Icons {
        layout: IconsLayout,
        items: Vec<VisibleItemSnapshot>,
    },
    Details {
        items: Vec<DetailsItemSnapshot>,
        row_count: usize,
        metrics: DetailsLayoutMetrics,
        name_column_width: f32,
    },
}

#[derive(Clone, Debug)]
pub(crate) enum FileGridRenderSnapshot {
    Compact {
        layout: CompactLayout,
        items: Vec<ItemPaintSnapshot>,
    },
    Icons {
        layout: IconsLayout,
        items: Vec<ItemPaintSnapshot>,
    },
    Details {
        items: Vec<DetailsPaintSnapshot>,
        row_count: usize,
        metrics: DetailsLayoutMetrics,
        name_column_width: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PaneViewportGeometry {
    pub(crate) window_rect: ViewRect,
}
