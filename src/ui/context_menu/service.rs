use fika_core::{ServiceMenuAction, ServiceMenuPriority};

use super::items::{context_menu_item, context_menu_submenu_item, disabled_context_menu_item};
use super::{ContextMenuAction, ContextMenuIcon, ContextMenuItem, ContextMenuSubmenu};

pub(super) fn service_menu_root_actions(actions: &[ServiceMenuAction]) -> Vec<ContextMenuItem> {
    actions
        .iter()
        .filter(|action| service_menu_action_promoted(action, actions.len()))
        .map(service_menu_action_item)
        .collect()
}

pub(super) fn service_menu_has_more_actions(actions: &[ServiceMenuAction]) -> bool {
    actions
        .iter()
        .any(|action| !service_menu_action_promoted(action, actions.len()))
}

pub(super) fn service_menu_more_actions(actions: &[ServiceMenuAction]) -> Vec<ContextMenuItem> {
    if actions.is_empty() {
        return vec![disabled_context_menu_item(
            ContextMenuAction::ServiceMenuSubmenu,
            "No Actions",
        )];
    }
    let more_actions = service_menu_more_action_refs(actions);
    if more_actions.is_empty() {
        return vec![disabled_context_menu_item(
            ContextMenuAction::ServiceMenuSubmenu,
            "No More Actions",
        )];
    }

    let (ungrouped, groups) = service_menu_partition_grouped_actions(more_actions);
    let mut items = ungrouped
        .into_iter()
        .map(service_menu_action_item)
        .collect::<Vec<_>>();
    for (group_index, (label, _)) in groups.iter().enumerate() {
        let mut group_item = context_menu_submenu_item(
            ContextMenuAction::ServiceMenuGroupSubmenu { group_index },
            label.clone(),
            ContextMenuSubmenu::ServiceMenuGroup(group_index),
        );
        group_item.separator_before = !items.is_empty() && group_index == 0;
        items.push(group_item);
    }
    items
}

pub(super) fn service_menu_group_actions(
    actions: &[ServiceMenuAction],
    group_index: usize,
) -> Vec<ContextMenuItem> {
    let more_actions = service_menu_more_action_refs(actions);
    let (_, groups) = service_menu_partition_grouped_actions(more_actions);
    let Some((_, group_actions)) = groups.into_iter().nth(group_index) else {
        return vec![disabled_context_menu_item(
            ContextMenuAction::ServiceMenuGroupSubmenu { group_index },
            "No Actions",
        )];
    };
    group_actions
        .into_iter()
        .map(service_menu_action_item)
        .collect()
}

fn service_menu_more_action_refs(actions: &[ServiceMenuAction]) -> Vec<&ServiceMenuAction> {
    actions
        .iter()
        .filter(|action| !service_menu_action_promoted(action, actions.len()))
        .collect()
}

fn service_menu_partition_grouped_actions(
    actions: Vec<&ServiceMenuAction>,
) -> (
    Vec<&ServiceMenuAction>,
    Vec<(String, Vec<&ServiceMenuAction>)>,
) {
    let mut grouped: Vec<(String, Vec<&ServiceMenuAction>)> = Vec::new();
    let ungrouped = actions
        .iter()
        .copied()
        .filter(|action| action.submenu.is_none())
        .collect::<Vec<_>>();

    for action in actions
        .into_iter()
        .filter(|action| action.submenu.is_some())
    {
        let group = action.submenu.as_deref().unwrap_or_default().to_string();
        if let Some((_, group_actions)) = grouped
            .iter_mut()
            .find(|(existing, _)| existing.eq_ignore_ascii_case(&group))
        {
            group_actions.push(action);
        } else {
            grouped.push((group, vec![action]));
        }
    }

    (ungrouped, grouped)
}

fn service_menu_action_promoted(action: &ServiceMenuAction, action_count: usize) -> bool {
    if action.priority == ServiceMenuPriority::TopLevel {
        return true;
    }
    if action.submenu.is_some() {
        return false;
    }
    if action_count <= 4 {
        return true;
    }
    let label = action.label.to_ascii_lowercase();
    [
        "compress", "extract", "archive", "terminal", "send to", "copy to", "move to",
    ]
    .iter()
    .any(|keyword| label.contains(keyword))
}

pub(super) fn should_offer_compress_fallback(actions: &[ServiceMenuAction]) -> bool {
    !actions.iter().any(service_menu_action_is_compress)
}

fn service_menu_action_is_compress(action: &ServiceMenuAction) -> bool {
    let label = action.label.to_ascii_lowercase();
    let id = action.id.to_ascii_lowercase();
    label.contains("compress")
        || id.contains("compress")
        || label.contains("create archive")
        || id.contains("create-archive")
        || id.contains("create_archive")
}

pub(super) fn should_offer_extract_fallback(actions: &[ServiceMenuAction]) -> bool {
    !actions.iter().any(service_menu_action_is_extract)
}

fn service_menu_action_is_extract(action: &ServiceMenuAction) -> bool {
    let label = action.label.to_ascii_lowercase();
    let id = action.id.to_ascii_lowercase();
    label.contains("extract")
        || id.contains("extract")
        || label.contains("unarchive")
        || id.contains("unarchive")
}

fn service_menu_action_item(action: &ServiceMenuAction) -> ContextMenuItem {
    let mut item = context_menu_item(
        ContextMenuAction::RunServiceMenuAction {
            action_id: action.id.clone(),
        },
        action.label.clone(),
    );
    if let Some(icon) = action.icon.as_ref().filter(|icon| !icon.trim().is_empty()) {
        item.icon = Some(ContextMenuIcon::Named(icon.trim().to_string()));
    }
    item
}
