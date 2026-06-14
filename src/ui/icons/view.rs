use gpui::prelude::*;
use gpui::{AnyElement, img};

use super::FileIconSnapshot;

pub(crate) fn cached_icon_or_fallback(
    icon: &FileIconSnapshot,
    fallback: impl Fn() -> AnyElement + 'static,
) -> AnyElement {
    match icon.path.clone() {
        Some(path) => img(path)
            .size_full()
            .with_fallback(fallback)
            .into_any_element(),
        None => fallback(),
    }
}
