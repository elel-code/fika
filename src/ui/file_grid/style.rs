use fika_core::ItemId;
use gpui::{Rgba, rgb, rgba};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum ItemTileTextAlignment {
    Start,
    Center,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct TextShapeCacheStats {
    pub(super) hits: usize,
    pub(super) misses: usize,
    pub(super) evicted: usize,
    pub(super) entries: usize,
}

impl TextShapeCacheStats {
    pub(super) fn has_activity(self) -> bool {
        self.hits > 0 || self.misses > 0 || self.evicted > 0
    }
}

pub(super) fn item_identity_element_id(
    prefix: &'static str,
    item_id: ItemId,
) -> (&'static str, u64) {
    (prefix, item_id.0)
}

pub(super) fn details_row_background(
    selected: bool,
    hovered: bool,
    drop_target: bool,
    row_index: usize,
) -> Rgba {
    if drop_target {
        drop_target_item_background()
    } else if selected && hovered {
        rgb(0xcfe3ff)
    } else if selected {
        rgb(0xdbeafe)
    } else if hovered {
        rgb(0xeaf1ff)
    } else if row_index % 2 == 0 {
        rgb(0xffffff)
    } else {
        rgb(0xf8fafc)
    }
}

pub(super) fn item_tile_background(selected: bool, drop_target: bool, hovered: bool) -> Rgba {
    if drop_target {
        drop_target_item_background()
    } else if selected && hovered {
        rgb(0xcfe3ff)
    } else if selected {
        rgb(0xdbeafe)
    } else if hovered {
        rgb(0xeaf1ff)
    } else {
        rgba(0x00000000)
    }
}

fn drop_target_item_background() -> Rgba {
    rgba(0xf59e0b4a)
}
