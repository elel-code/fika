mod cache;
mod roles;
mod view;

pub(crate) use cache::{FileIconCache, FileIconRenderResult, FileIconSnapshot};
pub(crate) use roles::{
    file_icon_snapshot_for_model_role, finish_mime_probe_results_with_icon_roles,
};
pub(crate) use view::cached_icon_or_fallback;
