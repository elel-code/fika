mod cache;
mod view;

pub(crate) use cache::{FileIconCache, FileIconSnapshot};
pub(crate) use view::cached_icon_or_fallback;
