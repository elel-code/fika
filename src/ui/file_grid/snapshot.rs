use std::path::PathBuf;
use std::sync::Arc;

use crate::ui::drag_drop::FileTransferMode;
use crate::ui::icons::FileIconSnapshot;

#[derive(Clone, Debug)]
pub(crate) struct VisibleItemSnapshot {
    pub(crate) slot_id: u64,
    pub(crate) layout: fika_core::ItemLayout,
    pub(crate) path: PathBuf,
    pub(crate) is_dir: bool,
    pub(crate) name: Arc<str>,
    pub(crate) kind_label: String,
    pub(crate) thumbnail_path: Option<PathBuf>,
    pub(crate) icon: FileIconSnapshot,
    pub(crate) selected: bool,
    pub(crate) selection_count: usize,
    pub(crate) drop_target: Option<FileTransferMode>,
    pub(crate) draft_name: Option<String>,
}
