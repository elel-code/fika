use super::{ContextMenuAction, ContextMenuIcon, ContextMenuItem, ContextMenuSubmenu};

pub(super) fn context_menu_item(
    action: ContextMenuAction,
    label: impl Into<String>,
) -> ContextMenuItem {
    let icon = context_menu_icon_for_action(&action);
    ContextMenuItem {
        action,
        label: label.into(),
        enabled: true,
        submenu: None,
        icon,
        separator_before: false,
    }
}

pub(super) fn context_menu_item_enabled(
    action: ContextMenuAction,
    label: impl Into<String>,
    enabled: bool,
) -> ContextMenuItem {
    let icon = context_menu_icon_for_action(&action);
    ContextMenuItem {
        action,
        label: label.into(),
        enabled,
        submenu: None,
        icon,
        separator_before: false,
    }
}

pub(super) fn context_menu_submenu_item(
    action: ContextMenuAction,
    label: impl Into<String>,
    submenu: ContextMenuSubmenu,
) -> ContextMenuItem {
    let icon = context_menu_icon_for_action(&action);
    ContextMenuItem {
        action,
        label: label.into(),
        enabled: true,
        submenu: Some(submenu),
        icon,
        separator_before: false,
    }
}

pub(super) fn disabled_context_menu_item(
    action: ContextMenuAction,
    label: impl Into<String>,
) -> ContextMenuItem {
    let icon = context_menu_icon_for_action(&action);
    ContextMenuItem {
        action,
        label: label.into(),
        enabled: false,
        submenu: None,
        icon,
        separator_before: false,
    }
}

pub(super) fn context_menu_separator_before(mut item: ContextMenuItem) -> ContextMenuItem {
    item.separator_before = true;
    item
}

pub(super) fn context_menu_group_items(mut items: Vec<ContextMenuItem>) -> Vec<ContextMenuItem> {
    if let Some(first) = items.first_mut() {
        first.separator_before = true;
    }
    items
}

fn context_menu_icon_for_action(action: &ContextMenuAction) -> Option<ContextMenuIcon> {
    match action {
        ContextMenuAction::Open | ContextMenuAction::OpenInNewPane => Some(ContextMenuIcon::Open),
        ContextMenuAction::OpenInNewWindow => Some(ContextMenuIcon::NewWindow),
        ContextMenuAction::OpenWithSubmenu => Some(ContextMenuIcon::OpenWith),
        ContextMenuAction::OpenWithApplication { .. } | ContextMenuAction::OtherApplication => {
            Some(ContextMenuIcon::Application)
        }
        ContextMenuAction::CreateNewSubmenu => Some(ContextMenuIcon::CreateNew),
        ContextMenuAction::ServiceMenuSubmenu
        | ContextMenuAction::ServiceMenuGroupSubmenu { .. }
        | ContextMenuAction::RunServiceMenuAction { .. } => Some(ContextMenuIcon::Service),
        ContextMenuAction::CompressWithArk
        | ContextMenuAction::ExtractHereWithArk
        | ContextMenuAction::ExtractToWithArk => Some(ContextMenuIcon::Archive),
        ContextMenuAction::MountDevice => Some(ContextMenuIcon::Named("media-mount".to_string())),
        ContextMenuAction::UnmountDevice => Some(ContextMenuIcon::Named("media-eject".to_string())),
        ContextMenuAction::EjectDevice => Some(ContextMenuIcon::Named("media-eject".to_string())),
        ContextMenuAction::SafelyRemoveDevice => {
            Some(ContextMenuIcon::Named("drive-removable-media".to_string()))
        }
        ContextMenuAction::AddPlace | ContextMenuAction::AddNetworkDrive => {
            Some(ContextMenuIcon::Place)
        }
        ContextMenuAction::EditPlace => Some(ContextMenuIcon::Edit),
        ContextMenuAction::RemovePlace => Some(ContextMenuIcon::Remove),
        ContextMenuAction::HidePlace
        | ContextMenuAction::HidePlaceSection
        | ContextMenuAction::ShowHiddenPlaces => Some(ContextMenuIcon::Hide),
        ContextMenuAction::SortBySubmenu
        | ContextMenuAction::SortByName
        | ContextMenuAction::SortByModified
        | ContextMenuAction::SortBySize
        | ContextMenuAction::SortByOriginalPath
        | ContextMenuAction::SortByDeletionTime
        | ContextMenuAction::SortAscending
        | ContextMenuAction::SortDescending
        | ContextMenuAction::SortFoldersFirst
        | ContextMenuAction::SortHiddenLast => Some(ContextMenuIcon::Sort),
        ContextMenuAction::ViewModeSubmenu
        | ContextMenuAction::ViewCompact
        | ContextMenuAction::ViewIcons
        | ContextMenuAction::ViewDetails => Some(ContextMenuIcon::View),
        ContextMenuAction::Rename => Some(ContextMenuIcon::Rename),
        ContextMenuAction::RenameAsAdministrator
        | ContextMenuAction::TrashAsAdministrator
        | ContextMenuAction::CreateFolderAsAdministrator
        | ContextMenuAction::CreateFileAsAdministrator
        | ContextMenuAction::PasteAsAdministrator => Some(ContextMenuIcon::Administrator),
        ContextMenuAction::Copy | ContextMenuAction::DropCopy => Some(ContextMenuIcon::Copy),
        ContextMenuAction::CopyLocation => Some(ContextMenuIcon::Location),
        ContextMenuAction::Cut | ContextMenuAction::DropMove => Some(ContextMenuIcon::Cut),
        ContextMenuAction::DropLink => Some(ContextMenuIcon::Link),
        ContextMenuAction::DropCancel => None,
        ContextMenuAction::Trash | ContextMenuAction::EmptyTrash => Some(ContextMenuIcon::Trash),
        ContextMenuAction::RestoreFromTrash => Some(ContextMenuIcon::Restore),
        ContextMenuAction::DeletePermanently => Some(ContextMenuIcon::Delete),
        ContextMenuAction::Properties => Some(ContextMenuIcon::Properties),
        ContextMenuAction::CreateFolder => Some(ContextMenuIcon::NewFolder),
        ContextMenuAction::CreateFile => Some(ContextMenuIcon::NewFile),
        ContextMenuAction::Paste => Some(ContextMenuIcon::Paste),
        ContextMenuAction::SelectAll => Some(ContextMenuIcon::Select),
        ContextMenuAction::Refresh => Some(ContextMenuIcon::Refresh),
    }
}
