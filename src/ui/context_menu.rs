use std::path::PathBuf;
use std::sync::Arc;

use fika_core::{MimeApplication, PaneId, ServiceMenuAction, ViewPoint};

mod actions;
mod icons;
mod items;
mod layout;
mod overlay;
mod service;

pub(crate) use actions::{context_menu_actions, context_submenu_actions};
pub(crate) use icons::context_menu_icon_snapshots;
#[cfg(test)]
pub(crate) use layout::{
    CONTEXT_MENU_ROW_HEIGHT, CONTEXT_MENU_VERTICAL_PADDING, CONTEXT_MENU_VIEWPORT_MARGIN,
};
pub(crate) use overlay::context_menu_overlay;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContextMenuSubmenu {
    CreateNew,
    OpenWith,
    ServiceMenu,
    ServiceMenuGroup(usize),
    SortBy,
    TrashSortBy,
    ViewMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ContextMenuOpenSubmenu {
    pub(crate) submenu: ContextMenuSubmenu,
    pub(crate) parent_index: usize,
    pub(crate) nested: Option<ContextMenuNestedSubmenu>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ContextMenuNestedSubmenu {
    pub(crate) submenu: ContextMenuSubmenu,
    pub(crate) parent_index: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ContextMenuState {
    pub(crate) pane_id: PaneId,
    pub(crate) target: ContextMenuTarget,
    pub(crate) position: ViewPoint,
    pub(crate) active_submenu: Option<ContextMenuOpenSubmenu>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ContextMenuTarget {
    Blank {
        trash_view: bool,
        trash_has_items: bool,
        service_actions: Vec<ServiceMenuAction>,
    },
    PlacesBlank {
        has_hidden_places: bool,
    },
    PlaceSection {
        group: &'static str,
    },
    DropOperation {
        target_dir: PathBuf,
        paths: Vec<PathBuf>,
        load_target_dir: bool,
    },
    Place {
        label: String,
        path: PathBuf,
        device_id: Option<String>,
        mounted: bool,
        device: bool,
        device_ejectable: bool,
        device_can_power_off: bool,
        trash_place: bool,
        trash_has_items: bool,
        editable: bool,
        removable: bool,
    },
    Item {
        path: PathBuf,
        is_dir: bool,
        selection_count: usize,
        trash_view: bool,
        trash_can_restore: bool,
        mime_type: Option<Arc<str>>,
        open_with_apps: Vec<MimeApplication>,
        service_actions: Vec<ServiceMenuAction>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ContextMenuAction {
    Open,
    OpenInNewPane,
    OpenInNewWindow,
    OpenWithSubmenu,
    OpenWithApplication { desktop_id: String },
    OtherApplication,
    CreateNewSubmenu,
    ServiceMenuSubmenu,
    ServiceMenuGroupSubmenu { group_index: usize },
    RunServiceMenuAction { action_id: String },
    CompressWithArk,
    ExtractHereWithArk,
    ExtractToWithArk,
    MountDevice,
    UnmountDevice,
    EjectDevice,
    SafelyRemoveDevice,
    AddPlace,
    EditPlace,
    RemovePlace,
    HidePlace,
    HidePlaceSection,
    ShowHiddenPlaces,
    SortBySubmenu,
    ViewModeSubmenu,
    SortByName,
    SortByModified,
    SortBySize,
    SortByOriginalPath,
    SortByDeletionTime,
    SortAscending,
    SortDescending,
    SortFoldersFirst,
    SortHiddenLast,
    ViewCompact,
    ViewIcons,
    ViewDetails,
    Rename,
    Copy,
    CopyLocation,
    Cut,
    Trash,
    RestoreFromTrash,
    DeletePermanently,
    EmptyTrash,
    Properties,
    CreateFolder,
    CreateFile,
    Paste,
    SelectAll,
    Refresh,
    DropCopy,
    DropMove,
    DropLink,
    DropCancel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ContextMenuItem {
    pub(crate) action: ContextMenuAction,
    pub(crate) label: String,
    pub(crate) enabled: bool,
    pub(crate) submenu: Option<ContextMenuSubmenu>,
    pub(crate) icon: Option<ContextMenuIcon>,
    pub(crate) separator_before: bool,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ContextMenuIcon {
    Named(String),
    Open,
    NewWindow,
    OpenWith,
    Application,
    Service,
    Archive,
    CreateNew,
    NewFolder,
    NewFile,
    Edit,
    Remove,
    Hide,
    Sort,
    View,
    Rename,
    Copy,
    Cut,
    Paste,
    Location,
    Trash,
    Restore,
    Delete,
    Properties,
    Select,
    Refresh,
    Place,
    Link,
}

#[cfg(test)]
pub(crate) fn context_menu_overlay_layout(
    position: ViewPoint,
    action_count: usize,
    active_submenu: Option<ContextMenuOpenSubmenu>,
    submenu_count: usize,
    nested_submenu_count: usize,
    viewport_width: f32,
    viewport_height: f32,
) -> layout::ContextMenuOverlayLayout {
    layout::context_menu_overlay_layout(
        position,
        action_count,
        active_submenu,
        submenu_count,
        nested_submenu_count,
        viewport_width,
        viewport_height,
    )
}
