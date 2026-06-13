use crate::ui::drag_drop::FileTransferMode;
use crate::ui::file_grid::VisibleItemSnapshot;
use crate::ui::filter_bar::FilterBarSnapshot;
use crate::ui::location_bar::LocationDraftSnapshot;
use crate::ui::status_bar::StatusBarSnapshot;
use fika_core::{BreadcrumbSegment, CompactLayout, PaneId, ViewRect, ViewState};

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
    pub(crate) view: ViewState,
    pub(crate) rubber_band: Option<ViewRect>,
    pub(crate) drop_target: Option<FileTransferMode>,
    pub(crate) focused: bool,
}
