use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::details::DetailsLayoutMetrics;
use super::super::layout::CompactColumnWidthCache;
use crate::ui::drag_drop::ItemDropTarget;
use crate::ui::rename::RenameDraft;

use fika_core::{
    CompactLayout, DirectoryModel, FilteredModel, IconsLayout, ItemId, ItemLayout, PaneId,
    SelectionState, ViewState,
};

#[derive(Clone, Debug)]
pub(crate) struct RawVisibleItemSnapshot {
    pub(crate) slot_id: u64,
    pub(crate) visible: bool,
    pub(crate) layout: ItemLayout,
    pub(crate) item_id: ItemId,
    pub(crate) path: PathBuf,
    pub(crate) is_dir: bool,
    pub(crate) name: Arc<str>,
    pub(crate) thumbnail_path: Option<PathBuf>,
    pub(crate) thumbnail_failed: bool,
    pub(crate) modified_secs: Option<u64>,
    pub(crate) size_bytes: u64,
    pub(crate) metadata_complete: bool,
    pub(crate) metadata_refresh_pending: bool,
    pub(crate) mime_type: Option<Arc<str>>,
    pub(crate) mime_magic_checked: bool,
    pub(crate) selected: bool,
    pub(crate) drop_target: bool,
    pub(crate) draft_name: Option<String>,
    pub(crate) draft_caret: Option<usize>,
    pub(crate) draft_selection: Option<(usize, usize)>,
    pub(crate) draft_error: Option<String>,
    pub(crate) draft_warning: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct RawDetailsItemSnapshot {
    pub(crate) row_index: usize,
    pub(crate) item_id: ItemId,
    pub(crate) path: PathBuf,
    pub(crate) is_dir: bool,
    pub(crate) name: Arc<str>,
    pub(crate) size_bytes: u64,
    pub(crate) modified_secs: Option<u64>,
    pub(crate) mime_type: Option<Arc<str>>,
    pub(crate) mime_magic_checked: bool,
    pub(crate) selected: bool,
    pub(crate) drop_target: bool,
    pub(crate) size_label: String,
    pub(crate) modified_label: String,
    pub(crate) original_path_label: String,
    pub(crate) deletion_time_label: String,
}

#[derive(Clone, Debug)]
pub(crate) enum RawFileGridSnapshot {
    Compact {
        layout: CompactLayout,
        items: Vec<RawVisibleItemSnapshot>,
    },
    Icons {
        layout: IconsLayout,
        items: Vec<RawVisibleItemSnapshot>,
    },
    Details {
        items: Vec<RawDetailsItemSnapshot>,
        row_count: usize,
        metrics: DetailsLayoutMetrics,
        name_column_width: f32,
    },
}

pub(crate) struct FileGridIconRequest<'a> {
    pub(crate) path: &'a Path,
    pub(crate) is_dir: bool,
    pub(crate) mime_type: Option<Arc<str>>,
    pub(crate) mime_magic_checked: bool,
    pub(crate) icon_size: f32,
}

pub(crate) struct RawFileGridSnapshotInput<'a> {
    pub(crate) pane_id: PaneId,
    pub(crate) model: &'a DirectoryModel,
    pub(crate) selection: &'a SelectionState,
    pub(crate) view: &'a ViewState,
    pub(crate) filtered: Option<&'a FilteredModel>,
    pub(crate) source_revision: u64,
    pub(crate) rename_draft: Option<&'a RenameDraft>,
    pub(crate) item_drop_target: Option<&'a ItemDropTarget>,
    pub(crate) compact_column_widths: &'a mut CompactColumnWidthCache,
}
