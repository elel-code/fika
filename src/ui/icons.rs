mod cache;
mod view;

pub(crate) use cache::{
    FileIconCache, FileIconResolveRequest, FileIconSnapshot, file_icon_resolve_results_for_requests,
};
pub(crate) use view::cached_icon_or_fallback;
