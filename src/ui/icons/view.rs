use gpui::prelude::*;
use gpui::{AnyElement, img};

use super::FileIconSnapshot;

pub(crate) fn cached_icon_or_fallback(
    icon: &FileIconSnapshot,
    fallback: impl Fn() -> AnyElement + 'static,
) -> AnyElement {
    match icon.render_image.clone() {
        Some(image) => img(image).size_full().into_any_element(),
        None => fallback(),
    }
}
