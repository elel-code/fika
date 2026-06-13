use std::path::PathBuf;

use fika_core::PaneId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RenameDraft {
    pub(crate) pane_id: PaneId,
    pub(crate) original_path: PathBuf,
    pub(crate) draft_name: String,
    pub(crate) error: Option<String>,
}
