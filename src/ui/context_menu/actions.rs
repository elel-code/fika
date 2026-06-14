use std::collections::HashSet;

use fika_core::{MimeApplication, is_archive_mime_or_path};

use super::items::{
    context_menu_group_items, context_menu_item, context_menu_item_enabled,
    context_menu_separator_before, context_menu_submenu_item, disabled_context_menu_item,
};
use super::service::{
    service_menu_group_actions, service_menu_has_more_actions, service_menu_more_actions,
    service_menu_root_actions, should_offer_compress_fallback, should_offer_extract_fallback,
};
use super::{
    ContextMenuAction, ContextMenuIcon, ContextMenuItem, ContextMenuSubmenu, ContextMenuTarget,
};

pub(crate) fn context_menu_actions(
    target: &ContextMenuTarget,
    clipboard_available: bool,
) -> Vec<ContextMenuItem> {
    match target {
        ContextMenuTarget::Blank {
            trash_view: true,
            trash_has_items,
            ..
        } => vec![
            context_menu_item_enabled(
                ContextMenuAction::EmptyTrash,
                "Empty Trash",
                *trash_has_items,
            ),
            context_menu_separator_before(context_menu_submenu_item(
                ContextMenuAction::SortBySubmenu,
                "Sort By",
                ContextMenuSubmenu::TrashSortBy,
            )),
            context_menu_submenu_item(
                ContextMenuAction::ViewModeSubmenu,
                "View Mode",
                ContextMenuSubmenu::ViewMode,
            ),
            context_menu_separator_before(context_menu_item(
                ContextMenuAction::SelectAll,
                "Select All",
            )),
            context_menu_item(ContextMenuAction::Refresh, "Refresh"),
            context_menu_separator_before(context_menu_item(
                ContextMenuAction::Properties,
                "Properties",
            )),
        ],
        ContextMenuTarget::Blank {
            trash_view: false,
            service_actions,
            ..
        } => {
            let mut actions = vec![
                context_menu_submenu_item(
                    ContextMenuAction::CreateNewSubmenu,
                    "Create New",
                    ContextMenuSubmenu::CreateNew,
                ),
                context_menu_separator_before(context_menu_item_enabled(
                    ContextMenuAction::Paste,
                    "Paste",
                    clipboard_available,
                )),
            ];
            let service_root_actions =
                context_menu_group_items(service_menu_root_actions(service_actions));
            let has_service_root_actions = !service_root_actions.is_empty();
            actions.extend(service_root_actions);
            if service_menu_has_more_actions(service_actions) {
                let more_actions = context_menu_submenu_item(
                    ContextMenuAction::ServiceMenuSubmenu,
                    "More Actions",
                    ContextMenuSubmenu::ServiceMenu,
                );
                actions.push(if has_service_root_actions {
                    more_actions
                } else {
                    context_menu_separator_before(more_actions)
                });
            }
            actions.extend([
                context_menu_separator_before(context_menu_submenu_item(
                    ContextMenuAction::SortBySubmenu,
                    "Sort By",
                    ContextMenuSubmenu::SortBy,
                )),
                context_menu_submenu_item(
                    ContextMenuAction::ViewModeSubmenu,
                    "View Mode",
                    ContextMenuSubmenu::ViewMode,
                ),
                context_menu_separator_before(context_menu_item(
                    ContextMenuAction::SelectAll,
                    "Select All",
                )),
                context_menu_item(ContextMenuAction::Refresh, "Refresh"),
                context_menu_separator_before(context_menu_item(
                    ContextMenuAction::Properties,
                    "Properties",
                )),
            ]);
            actions
        }
        ContextMenuTarget::PlacesBlank { has_hidden_places } => {
            let mut actions = vec![context_menu_item(ContextMenuAction::AddPlace, "Add Entry")];
            actions.push(context_menu_item_enabled(
                ContextMenuAction::ShowHiddenPlaces,
                "Show Hidden Places",
                *has_hidden_places,
            ));
            actions
        }
        ContextMenuTarget::PlaceSection { .. } => {
            vec![context_menu_item(
                ContextMenuAction::HidePlaceSection,
                "Hide Section",
            )]
        }
        ContextMenuTarget::Place {
            mounted,
            trash_place: true,
            trash_has_items,
            ..
        } => vec![
            context_menu_item_enabled(ContextMenuAction::Open, "Open", *mounted),
            context_menu_item_enabled(
                ContextMenuAction::OpenInNewPane,
                "Open in New Pane",
                *mounted,
            ),
            context_menu_item_enabled(
                ContextMenuAction::OpenInNewWindow,
                "Open in New Window",
                *mounted,
            ),
            context_menu_item_enabled(
                ContextMenuAction::EmptyTrash,
                "Empty Trash",
                *trash_has_items,
            ),
            context_menu_item(ContextMenuAction::HidePlace, "Hide"),
            context_menu_item(ContextMenuAction::CopyLocation, "Copy Location"),
            context_menu_separator_before(context_menu_item(
                ContextMenuAction::Properties,
                "Properties",
            )),
        ],
        ContextMenuTarget::Place {
            mounted,
            device,
            device_ejectable,
            device_can_power_off,
            editable,
            removable,
            ..
        } => {
            let mut actions = vec![
                context_menu_item_enabled(ContextMenuAction::Open, "Open", *mounted),
                context_menu_item_enabled(
                    ContextMenuAction::OpenInNewPane,
                    "Open in New Pane",
                    *mounted,
                ),
                context_menu_item_enabled(
                    ContextMenuAction::OpenInNewWindow,
                    "Open in New Window",
                    *mounted,
                ),
            ];
            if *device {
                let mut device_actions = Vec::new();
                if *mounted {
                    device_actions.push(context_menu_item(
                        ContextMenuAction::UnmountDevice,
                        "Unmount",
                    ));
                } else {
                    device_actions.push(context_menu_item(ContextMenuAction::MountDevice, "Mount"));
                }
                if *device_ejectable {
                    device_actions.push(context_menu_item(ContextMenuAction::EjectDevice, "Eject"));
                }
                if *device_can_power_off {
                    device_actions.push(context_menu_item(
                        ContextMenuAction::SafelyRemoveDevice,
                        "Safely Remove",
                    ));
                }
                if !device_actions.is_empty() {
                    actions.extend(context_menu_group_items(device_actions));
                }
            }
            actions.extend([
                context_menu_item_enabled(ContextMenuAction::EditPlace, "Edit Entry", *editable),
                context_menu_item_enabled(
                    ContextMenuAction::RemovePlace,
                    "Remove Entry",
                    *removable,
                ),
                context_menu_item(ContextMenuAction::HidePlace, "Hide"),
                context_menu_item(ContextMenuAction::CopyLocation, "Copy Location"),
                context_menu_separator_before(context_menu_item(
                    ContextMenuAction::Properties,
                    "Properties",
                )),
            ]);
            actions
        }
        ContextMenuTarget::Item {
            trash_view: true,
            trash_can_restore,
            ..
        } => vec![
            context_menu_item_enabled(
                ContextMenuAction::RestoreFromTrash,
                "Restore to Former Location",
                *trash_can_restore,
            ),
            context_menu_item(ContextMenuAction::Copy, "Copy"),
            context_menu_item(ContextMenuAction::DeletePermanently, "Delete Permanently"),
            context_menu_separator_before(context_menu_item(
                ContextMenuAction::Properties,
                "Properties",
            )),
        ],
        ContextMenuTarget::Item {
            selection_count,
            service_actions,
            ..
        } if *selection_count > 1 => {
            let mut actions = vec![
                context_menu_item(ContextMenuAction::Cut, "Cut"),
                context_menu_item(ContextMenuAction::Copy, "Copy"),
            ];
            let service_root_actions =
                context_menu_group_items(service_menu_root_actions(service_actions));
            let has_service_root_actions = !service_root_actions.is_empty();
            actions.extend(service_root_actions);
            if service_menu_has_more_actions(service_actions) {
                let more_actions = context_menu_submenu_item(
                    ContextMenuAction::ServiceMenuSubmenu,
                    "More Actions",
                    ContextMenuSubmenu::ServiceMenu,
                );
                actions.push(if has_service_root_actions {
                    more_actions
                } else {
                    context_menu_separator_before(more_actions)
                });
            }
            if should_offer_compress_fallback(service_actions) {
                actions.extend(context_menu_group_items(vec![context_menu_item(
                    ContextMenuAction::CompressWithArk,
                    "Compress...",
                )]));
            }
            actions.push(context_menu_separator_before(context_menu_item(
                ContextMenuAction::Trash,
                "Move to Trash",
            )));
            actions.push(context_menu_separator_before(context_menu_item(
                ContextMenuAction::Properties,
                "Properties",
            )));
            actions
        }
        ContextMenuTarget::Item {
            path,
            is_dir,
            mime_type,
            service_actions,
            open_with_apps,
            ..
        } => {
            let mut actions = if *is_dir {
                vec![context_menu_item(ContextMenuAction::Open, "Open")]
            } else {
                vec![context_menu_submenu_item(
                    ContextMenuAction::OpenWithSubmenu,
                    "Open With",
                    ContextMenuSubmenu::OpenWith,
                )]
            };
            if *is_dir {
                actions.push(context_menu_item(
                    ContextMenuAction::OpenInNewPane,
                    "Open in New Pane",
                ));
                actions.push(context_menu_item(
                    ContextMenuAction::OpenInNewWindow,
                    "Open in New Window",
                ));
                if !open_with_apps.is_empty() {
                    actions.push(context_menu_submenu_item(
                        ContextMenuAction::OpenWithSubmenu,
                        "Open With",
                        ContextMenuSubmenu::OpenWith,
                    ));
                }
                actions.push(context_menu_submenu_item(
                    ContextMenuAction::CreateNewSubmenu,
                    "Create New",
                    ContextMenuSubmenu::CreateNew,
                ));
            }
            actions.extend([
                context_menu_separator_before(context_menu_item(ContextMenuAction::Cut, "Cut")),
                context_menu_item(ContextMenuAction::Copy, "Copy"),
                context_menu_item(ContextMenuAction::CopyLocation, "Copy Location"),
            ]);
            if *is_dir {
                actions.push(context_menu_item_enabled(
                    ContextMenuAction::Paste,
                    "Paste",
                    clipboard_available,
                ));
            }
            let service_root_actions =
                context_menu_group_items(service_menu_root_actions(service_actions));
            let has_service_root_actions = !service_root_actions.is_empty();
            actions.extend(service_root_actions);
            if service_menu_has_more_actions(service_actions) {
                let more_actions = context_menu_submenu_item(
                    ContextMenuAction::ServiceMenuSubmenu,
                    "More Actions",
                    ContextMenuSubmenu::ServiceMenu,
                );
                actions.push(if has_service_root_actions {
                    more_actions
                } else {
                    context_menu_separator_before(more_actions)
                });
            }
            if should_offer_compress_fallback(service_actions)
                && (*is_dir || !is_archive_mime_or_path(mime_type.as_deref(), path))
            {
                actions.extend(context_menu_group_items(vec![context_menu_item(
                    ContextMenuAction::CompressWithArk,
                    "Compress...",
                )]));
            }
            if !*is_dir
                && is_archive_mime_or_path(mime_type.as_deref(), path)
                && should_offer_extract_fallback(service_actions)
            {
                actions.extend(context_menu_group_items(vec![
                    context_menu_item(ContextMenuAction::ExtractHereWithArk, "Extract Here"),
                    context_menu_item(ContextMenuAction::ExtractToWithArk, "Extract To..."),
                ]));
            }
            actions.extend([
                context_menu_separator_before(context_menu_item(
                    ContextMenuAction::Rename,
                    "Rename",
                )),
                context_menu_item(ContextMenuAction::Trash, "Move to Trash"),
                context_menu_separator_before(context_menu_item(
                    ContextMenuAction::Properties,
                    "Properties",
                )),
            ]);
            actions
        }
    }
}

pub(crate) fn context_submenu_actions(
    submenu: ContextMenuSubmenu,
    target: &ContextMenuTarget,
) -> Vec<ContextMenuItem> {
    match submenu {
        ContextMenuSubmenu::CreateNew => match target {
            ContextMenuTarget::Blank {
                trash_view: false, ..
            }
            | ContextMenuTarget::Item {
                is_dir: true,
                trash_view: false,
                ..
            } => vec![
                context_menu_item(ContextMenuAction::CreateFolder, "Folder"),
                context_menu_item(ContextMenuAction::CreateFile, "Text File"),
            ],
            _ => Vec::new(),
        },
        ContextMenuSubmenu::OpenWith => match target {
            ContextMenuTarget::Item { open_with_apps, .. } => {
                open_with_menu_actions(open_with_apps)
            }
            _ => Vec::new(),
        },
        ContextMenuSubmenu::ServiceMenu => match target {
            ContextMenuTarget::Blank {
                service_actions, ..
            }
            | ContextMenuTarget::Item {
                service_actions, ..
            } => service_menu_more_actions(service_actions),
            _ => Vec::new(),
        },
        ContextMenuSubmenu::ServiceMenuGroup(group_index) => match target {
            ContextMenuTarget::Blank {
                service_actions, ..
            }
            | ContextMenuTarget::Item {
                service_actions, ..
            } => service_menu_group_actions(service_actions, group_index),
            _ => Vec::new(),
        },
        ContextMenuSubmenu::SortBy => vec![
            context_menu_item(ContextMenuAction::SortByName, "Name"),
            context_menu_item(ContextMenuAction::SortByModified, "Modified"),
            context_menu_item(ContextMenuAction::SortBySize, "Size"),
            context_menu_item(ContextMenuAction::SortAscending, "Ascending"),
            context_menu_item(ContextMenuAction::SortDescending, "Descending"),
            context_menu_item(ContextMenuAction::SortFoldersFirst, "Folders First"),
            context_menu_item(ContextMenuAction::SortHiddenLast, "Hidden Files Last"),
        ],
        ContextMenuSubmenu::TrashSortBy => vec![
            context_menu_item(ContextMenuAction::SortByName, "Name"),
            context_menu_item(ContextMenuAction::SortByOriginalPath, "Original Path"),
            context_menu_item(ContextMenuAction::SortByDeletionTime, "Deletion Time"),
            context_menu_item(ContextMenuAction::SortAscending, "Ascending"),
            context_menu_item(ContextMenuAction::SortDescending, "Descending"),
            context_menu_item(ContextMenuAction::SortFoldersFirst, "Folders First"),
            context_menu_item(ContextMenuAction::SortHiddenLast, "Hidden Files Last"),
        ],
        ContextMenuSubmenu::ViewMode => vec![
            context_menu_item(ContextMenuAction::ViewCompact, "Compact"),
            context_menu_item(ContextMenuAction::ViewIcons, "Icons"),
            context_menu_item(ContextMenuAction::ViewDetails, "Details"),
        ],
    }
}

fn open_with_menu_actions(apps: &[MimeApplication]) -> Vec<ContextMenuItem> {
    let apps = dedup_open_with_apps(apps);
    let mut actions = if apps.is_empty() {
        vec![disabled_context_menu_item(
            ContextMenuAction::OpenWithSubmenu,
            "No Applications",
        )]
    } else {
        apps.into_iter()
            .map(|app| {
                let mut item = context_menu_item(
                    ContextMenuAction::OpenWithApplication {
                        desktop_id: app.id.clone(),
                    },
                    app.name.clone(),
                );
                if let Some(icon) = app.icon.as_ref().filter(|icon| !icon.trim().is_empty()) {
                    item.icon = Some(ContextMenuIcon::Named(icon.trim().to_string()));
                }
                item
            })
            .collect::<Vec<_>>()
    };
    actions.push(context_menu_item(
        ContextMenuAction::OtherApplication,
        "Other Application...",
    ));
    actions
}

fn dedup_open_with_apps(apps: &[MimeApplication]) -> Vec<&MimeApplication> {
    let mut seen_ids = HashSet::new();
    let mut seen_names = HashSet::new();
    let mut deduped = Vec::new();
    for app in apps {
        let id = app.id.to_ascii_lowercase();
        let name = app.name.trim().to_ascii_lowercase();
        if seen_ids.insert(id) && seen_names.insert(name) {
            deduped.push(app);
        }
    }
    deduped
}
