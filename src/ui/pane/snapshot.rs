use crate::ui::drag_drop::FileTransferMode;
use crate::ui::file_grid::{DetailsItemSnapshot, VisibleItemSnapshot};
use crate::ui::filter_bar::FilterBarSnapshot;
use crate::ui::location_bar::LocationDraftSnapshot;
use crate::ui::status_bar::StatusBarSnapshot;
use fika_core::{BreadcrumbSegment, CompactLayout, IconsLayout, PaneId, ViewRect, ViewState};
use gpui::ScrollHandle;

use super::toolbar::PaneToolbarSnapshot;

#[derive(Clone, Debug)]
pub(crate) struct PaneSnapshot {
    pub(crate) id: PaneId,
    pub(crate) split_ratio: f32,
    pub(crate) breadcrumbs: Vec<BreadcrumbSegment>,
    pub(crate) location_draft: Option<LocationDraftSnapshot>,
    pub(crate) filter_bar: Option<FilterBarSnapshot>,
    pub(crate) toolbar: PaneToolbarSnapshot,
    pub(crate) status_bar: StatusBarSnapshot,
    pub(crate) layout: CompactLayout,
    pub(crate) visible_items: Vec<VisibleItemSnapshot>,
    pub(crate) icons_layout: IconsLayout,
    pub(crate) icons_items: Vec<VisibleItemSnapshot>,
    pub(crate) details_items: Vec<DetailsItemSnapshot>,
    pub(crate) details_row_count: usize,
    pub(crate) trash_view: bool,
    pub(crate) scroll_handle: ScrollHandle,
    pub(crate) view: ViewState,
    pub(crate) rubber_band: Option<ViewRect>,
    pub(crate) drop_target: Option<FileTransferMode>,
    pub(crate) focused: bool,
}
