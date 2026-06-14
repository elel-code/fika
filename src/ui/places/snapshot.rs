use std::path::PathBuf;

use crate::ui::icons::FileIconSnapshot;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum PlaceIcon {
    Home,
    Desktop,
    Documents,
    Downloads,
    Music,
    Pictures,
    Videos,
    Trash,
    Root,
    Network,
    Device,
    Bookmark,
    Folder,
}

#[derive(Clone, Debug)]
pub(crate) struct PlaceSnapshot {
    pub(crate) index: usize,
    pub(crate) group: &'static str,
    pub(crate) icon: FileIconSnapshot,
    pub(crate) label: String,
    pub(crate) path: PathBuf,
    pub(crate) mounted: bool,
    pub(crate) device: bool,
    pub(crate) network: bool,
    pub(crate) device_ejectable: bool,
    pub(crate) device_can_power_off: bool,
    pub(crate) active: bool,
    pub(crate) drop_target: bool,
    pub(crate) insert_before: bool,
    pub(crate) insert_after: bool,
    pub(crate) trash_place: bool,
    pub(crate) trash_has_items: bool,
    pub(crate) editable: bool,
    pub(crate) removable: bool,
}
