use crate::app::async_bridge::{AsyncBridge, send_async_event};
use crate::app::events::{AsyncEvent, ServiceMenuActionLaunchResult};
use crate::app::split_view::sync_pane_slot_ui;
use crate::app::state::AppState;
use crate::config::service_menu_policy::{ServiceMenuPolicy, save_service_menu_policy};
use crate::desktop::service_menu;
use crate::{AppWindow, ContextServiceAction, ContextServicePolicyAction};
use slint::{ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

const SERVICE_ROW_ACTION: i32 = 0;
const SERVICE_ROW_SUBMENU: i32 = 1;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ServiceMenuRowCounts {
    action_rows: i32,
    submenu_rows: i32,
}

pub(crate) fn item_paths(
    state: &Rc<RefCell<AppState>>,
    slot: i32,
    context_path: &str,
) -> Vec<PathBuf> {
    let state = state.borrow();
    let Some(pane) = state.panes.pane_for_slot(slot) else {
        return vec![PathBuf::from(context_path)];
    };
    if pane.selection.paths.len() > 1
        && pane
            .selection
            .paths
            .iter()
            .any(|selected| selected == context_path)
    {
        pane.selection.paths.iter().map(PathBuf::from).collect()
    } else {
        vec![PathBuf::from(context_path)]
    }
}

pub(crate) fn blank_paths(state: &Rc<RefCell<AppState>>, slot: i32) -> Vec<PathBuf> {
    state
        .borrow()
        .panes
        .pane_for_slot(slot)
        .map(|pane| vec![pane.current_dir.clone()])
        .unwrap_or_default()
}

pub(crate) fn refresh_actions_async(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    slot: i32,
    paths: Vec<PathBuf>,
) {
    let generation = {
        let mut state = state.borrow_mut();
        let generation = state.service_menu_generation.next();
        state.context_service_menu_paths = paths.clone();
        state.context_service_menu_actions.clear();
        state.context_service_menu_all_actions.clear();
        state.context_service_menu_pane_id = state.panes.pane_for_slot(slot).map(|pane| pane.id);
        generation
    };
    sync_service_menu_ui(ui, &[], &[], &ServiceMenuPolicy::default());

    if paths.is_empty() {
        return;
    }

    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let query_paths = paths.clone();
        let result =
            tokio::task::spawn_blocking(move || service_menu::list_actions_for_paths(&query_paths))
                .await
                .unwrap_or_else(|err| Err(format!("service menu task failed: {err}")));
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::ServiceMenuActionsLoaded(service_menu::ServiceMenuActionsResult {
                generation,
                paths,
                result,
            }),
        );
    });
}

pub(crate) fn apply_actions_result(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    result: service_menu::ServiceMenuActionsResult,
) {
    let (visible_actions, all_actions, policy) = {
        let mut state = state.borrow_mut();
        if !state.service_menu_generation.is_current(result.generation)
            || state.context_service_menu_paths != result.paths
        {
            return;
        }
        state.context_service_menu_all_actions = result.result.unwrap_or_default();
        state.context_service_menu_actions = enabled_actions(
            &state.context_service_menu_all_actions,
            &state.service_menu_policy,
        );
        (
            state.context_service_menu_actions.clone(),
            state.context_service_menu_all_actions.clone(),
            state.service_menu_policy.clone(),
        )
    };
    sync_service_menu_ui(ui, &visible_actions, &all_actions, &policy);
}

fn sync_service_menu_ui(
    ui: &AppWindow,
    visible_actions: &[service_menu::ServiceMenuAction],
    all_actions: &[service_menu::ServiceMenuAction],
    policy: &ServiceMenuPolicy,
) {
    sync_actions_ui(ui, visible_actions, all_actions.len());
    sync_policy_actions_ui(ui, all_actions, policy);
}

fn sync_actions_ui(
    ui: &AppWindow,
    actions: &[service_menu::ServiceMenuAction],
    all_action_count: usize,
) {
    let (rows, counts) = menu_rows(actions);
    ui.set_context_service_actions(ModelRc::new(Rc::new(VecModel::from(rows))));
    ui.set_context_service_child_actions(empty_actions_model());
    ui.set_context_service_action_rows(counts.action_rows);
    ui.set_context_service_submenu_rows(counts.submenu_rows);
    ui.set_context_service_config_rows(i32::from(all_action_count > 0));
}

fn sync_policy_actions_ui(
    ui: &AppWindow,
    actions: &[service_menu::ServiceMenuAction],
    policy: &ServiceMenuPolicy,
) {
    ui.set_context_service_policy_actions(ModelRc::new(Rc::new(VecModel::from(
        policy_action_rows(actions, policy),
    ))));
}

fn menu_rows(
    actions: &[service_menu::ServiceMenuAction],
) -> (Vec<ContextServiceAction>, ServiceMenuRowCounts) {
    let mut rows = Vec::new();
    let mut counts = ServiceMenuRowCounts::default();
    let mut current_group: Option<&str> = None;

    for (index, action) in actions.iter().enumerate() {
        if action.top_level || action.submenu.is_empty() {
            rows.push(service_menu_action_row(action, index));
            counts.action_rows += 1;
            continue;
        }

        let group = action.submenu.as_str();
        if current_group != Some(group) {
            rows.push(service_menu_submenu_row(group));
            counts.submenu_rows += 1;
            current_group = Some(group);
        }
    }

    (rows, counts)
}

pub(crate) fn prepare_submenu_actions(ui: &AppWindow, state: &Rc<RefCell<AppState>>, group: &str) {
    let rows = {
        let state = state.borrow();
        child_menu_rows(&state.context_service_menu_actions, group)
    };
    ui.set_context_service_child_actions(ModelRc::new(Rc::new(VecModel::from(rows))));
}

pub(crate) fn set_action_enabled(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    action_id: &str,
    enabled: bool,
) {
    let (visible_actions, all_actions, policy, pane_id, save_result) = {
        let mut state = state.borrow_mut();
        let previous_policy = state.service_menu_policy.clone();
        let mut next_policy = previous_policy.clone();
        next_policy.set_enabled(action_id, enabled);
        let save_result = save_service_menu_policy(&next_policy);
        state.service_menu_policy = if save_result.is_ok() {
            next_policy
        } else {
            previous_policy
        };
        state.context_service_menu_actions = enabled_actions(
            &state.context_service_menu_all_actions,
            &state.service_menu_policy,
        );
        (
            state.context_service_menu_actions.clone(),
            state.context_service_menu_all_actions.clone(),
            state.service_menu_policy.clone(),
            state.context_service_menu_pane_id,
            save_result,
        )
    };

    sync_service_menu_ui(ui, &visible_actions, &all_actions, &policy);

    let Some(pane_id) = pane_id else {
        return;
    };
    match save_result {
        Ok(()) => set_status_for_pane_id(
            ui,
            state,
            pane_id,
            if enabled {
                "Service menu action enabled"
            } else {
                "Service menu action disabled"
            },
        ),
        Err(err) => set_status_for_pane_id(
            ui,
            state,
            pane_id,
            &format!("Cannot save service menu policy: {err}"),
        ),
    }
}

fn child_menu_rows(
    actions: &[service_menu::ServiceMenuAction],
    group: &str,
) -> Vec<ContextServiceAction> {
    actions
        .iter()
        .enumerate()
        .filter(|(_, action)| !action.top_level && action.submenu == group)
        .map(|(index, action)| service_menu_action_row(action, index))
        .collect()
}

fn service_menu_action_row(
    action: &service_menu::ServiceMenuAction,
    index: usize,
) -> ContextServiceAction {
    ContextServiceAction {
        id: action.id.clone().into(),
        name: action.name.clone().into(),
        group: action.submenu.clone().into(),
        action_index: index as i32,
        row_kind: SERVICE_ROW_ACTION,
    }
}

fn service_menu_submenu_row(group: &str) -> ContextServiceAction {
    ContextServiceAction {
        id: group.into(),
        name: group.into(),
        group: group.into(),
        action_index: -1,
        row_kind: SERVICE_ROW_SUBMENU,
    }
}

fn policy_action_rows(
    actions: &[service_menu::ServiceMenuAction],
    policy: &ServiceMenuPolicy,
) -> Vec<ContextServicePolicyAction> {
    actions
        .iter()
        .map(|action| ContextServicePolicyAction {
            id: action.id.clone().into(),
            name: action.name.clone().into(),
            group: action.submenu.clone().into(),
            enabled: policy.is_enabled(&action.id),
        })
        .collect()
}

fn enabled_actions(
    actions: &[service_menu::ServiceMenuAction],
    policy: &ServiceMenuPolicy,
) -> Vec<service_menu::ServiceMenuAction> {
    actions
        .iter()
        .filter(|action| policy.is_enabled(&action.id))
        .cloned()
        .collect()
}

fn empty_actions_model() -> ModelRc<ContextServiceAction> {
    ModelRc::new(Rc::new(VecModel::from(Vec::<ContextServiceAction>::new())))
}

pub(crate) fn launch_action_async(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    index: i32,
) {
    let (pane_id, action) = {
        let state_ref = state.borrow();
        let Some(pane_id) = state_ref.context_service_menu_pane_id else {
            return;
        };
        let action = usize::try_from(index)
            .ok()
            .and_then(|index| state_ref.context_service_menu_actions.get(index).cloned());
        (pane_id, action)
    };

    let Some(action) = action else {
        set_status_for_pane_id(
            ui,
            state,
            pane_id,
            "Context menu action is no longer available",
        );
        return;
    };

    set_status_for_pane_id(ui, state, pane_id, &format!("Running {}...", action.name));

    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let action_name = action.name.clone();
        let result = tokio::task::spawn_blocking(move || service_menu::launch_action(&action))
            .await
            .unwrap_or_else(|err| Err(format!("service menu launch task failed: {err}")));
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::ServiceMenuActionFinished(ServiceMenuActionLaunchResult {
                pane_id,
                action_name,
                result,
            }),
        );
    });
}

pub(crate) fn apply_launch_result(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    result: ServiceMenuActionLaunchResult,
) {
    match result.result {
        Ok(launch) => match (launch.unit, launch.diagnostic) {
            (Some(unit), _) => set_status_for_pane_id(
                ui,
                state,
                result.pane_id,
                &format!("Ran {} ({unit})", result.action_name),
            ),
            (None, Some(diagnostic)) => set_status_for_pane_id(
                ui,
                state,
                result.pane_id,
                &format!("Ran {}; {diagnostic}", result.action_name),
            ),
            (None, None) => set_status_for_pane_id(
                ui,
                state,
                result.pane_id,
                &format!("Ran {}", result.action_name),
            ),
        },
        Err(err) => set_status_for_pane_id(
            ui,
            state,
            result.pane_id,
            &format!("Cannot run {}: {err}", result.action_name),
        ),
    }
}

fn set_status_for_pane_id(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    pane_id: u64,
    message: &str,
) {
    let Some((slot, is_focused)) = ({
        let mut state = state.borrow_mut();
        state.panes.slot_for_id(pane_id).and_then(|slot| {
            let is_focused = state.panes.focused_slot() == slot;
            let pane = state.panes.pane_mut_for_slot(slot)?;
            pane.status = message.to_string();
            Some((slot, is_focused))
        })
    }) else {
        return;
    };

    if is_focused {
        ui.set_status(SharedString::from(message));
    }
    sync_pane_slot_ui(ui, state, slot);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_paths_use_multiselection_only_when_context_item_is_selected() {
        let state = Rc::new(RefCell::new(AppState::new(
            PathBuf::from("/tmp"),
            Vec::new(),
        )));
        state.borrow_mut().panes.focused_mut().selection.paths =
            vec!["/tmp/a.txt".to_string(), "/tmp/b.txt".to_string()];

        assert_eq!(
            item_paths(&state, 0, "/tmp/a.txt"),
            vec![PathBuf::from("/tmp/a.txt"), PathBuf::from("/tmp/b.txt")]
        );
        assert_eq!(
            item_paths(&state, 0, "/tmp/c.txt"),
            vec![PathBuf::from("/tmp/c.txt")]
        );
        assert_eq!(
            item_paths(&state, 99, "/tmp/missing.txt"),
            vec![PathBuf::from("/tmp/missing.txt")]
        );
    }

    #[test]
    fn blank_paths_use_the_source_pane_directory() {
        let state = Rc::new(RefCell::new(AppState::new(
            PathBuf::from("/tmp/source"),
            Vec::new(),
        )));

        assert_eq!(blank_paths(&state, 0), vec![PathBuf::from("/tmp/source")]);
        assert!(blank_paths(&state, 99).is_empty());
    }

    #[test]
    fn service_menu_actions_are_pane_routed_and_model_backed() {
        let source = include_str!("context_service_menu.rs");
        let refresh_body = source
            .split_once("pub(crate) fn refresh_actions_async(")
            .and_then(|(_, rest)| rest.split_once("pub(crate) fn apply_actions_result("))
            .map(|(body, _)| body)
            .expect("service menu refresh body should be present");
        let apply_body = source
            .split_once("pub(crate) fn apply_actions_result(")
            .and_then(|(_, rest)| rest.split_once("fn sync_actions_ui("))
            .map(|(body, _)| body)
            .expect("service menu apply body should be present");
        let launch_body = source
            .split_once("pub(crate) fn launch_action_async(")
            .and_then(|(_, rest)| rest.split_once("pub(crate) fn apply_launch_result("))
            .map(|(body, _)| body)
            .expect("service menu launch body should be present");
        let result_body = source
            .split_once("pub(crate) fn apply_launch_result(")
            .and_then(|(_, rest)| rest.split_once("fn set_status_for_pane_id("))
            .map(|(body, _)| body)
            .expect("service menu result body should be present");
        let status_body = source
            .split_once("fn set_status_for_pane_id(")
            .and_then(|(_, rest)| rest.split_once("#[cfg(test)]"))
            .map(|(body, _)| body)
            .expect("service menu status body should be present");

        assert!(
            refresh_body.contains(
                "state.context_service_menu_pane_id = state.panes.pane_for_slot(slot).map(|pane| pane.id);"
            ) && refresh_body.contains("state.context_service_menu_all_actions.clear();")
                && refresh_body.contains("sync_service_menu_ui(ui, &[], &[], &ServiceMenuPolicy::default());"),
            "opening a context menu should remember the source pane and clear stale service action rows before async discovery"
        );
        assert!(
            apply_body.contains("let (visible_actions, all_actions, policy) = {\n        let mut state = state.borrow_mut();")
                && apply_body.contains("state.context_service_menu_all_actions = result.result.unwrap_or_default();")
                && apply_body.contains("state.context_service_menu_actions = enabled_actions(")
                && apply_body.contains("state.context_service_menu_actions.clone()")
                && apply_body.contains("state.context_service_menu_all_actions.clone()")
                && apply_body.contains("state.service_menu_policy.clone()")
                && apply_body.contains("sync_service_menu_ui(ui, &visible_actions, &all_actions, &policy);"),
            "service menu discovery results should keep all actions for policy editing, filter the visible menu snapshot, release the borrow, then write Slint models"
        );
        assert!(
            launch_body.contains("state_ref.context_service_menu_pane_id")
                && launch_body.contains("state_ref.context_service_menu_actions")
                && launch_body.contains(".get(index)")
                && launch_body.contains(".cloned()")
                && launch_body.contains("set_status_for_pane_id(")
                && launch_body.contains("AsyncEvent::ServiceMenuActionFinished")
                && !launch_body.contains("PaneTarget::Focused"),
            "service menu actions should launch from the stored context snapshot and report start status to the source pane"
        );
        assert!(
            result_body.matches("set_status_for_pane_id(").count() == 4,
            "service menu launch results should report to the source pane"
        );
        assert!(
            status_body.contains("state.panes.slot_for_id(pane_id).and_then")
                && status_body.contains("sync_pane_slot_ui(ui, state, slot);"),
            "service menu status updates should target the source pane row and not fall back to the focused pane"
        );
    }

    #[test]
    fn service_menu_rows_group_submenu_actions_without_changing_action_indices() {
        let (rows, counts) = menu_rows(&[
            service_action("top", "Top Action", "", true),
            service_action("edit", "Edit A", "Edit", false),
            service_action("edit2", "Edit B", "Edit", false),
            service_action("tools", "Tool", "Tools", false),
        ]);

        assert_eq!(
            counts,
            ServiceMenuRowCounts {
                action_rows: 1,
                submenu_rows: 2,
            }
        );
        assert_eq!(
            rows.iter()
                .map(|row| (row.row_kind, row.name.to_string(), row.action_index))
                .collect::<Vec<_>>(),
            vec![
                (0, "Top Action".to_string(), 0),
                (1, "Edit".to_string(), -1),
                (1, "Tools".to_string(), -1),
            ]
        );
        assert_eq!(
            child_menu_rows(
                &[
                    service_action("top", "Top Action", "", true),
                    service_action("edit", "Edit A", "Edit", false),
                    service_action("edit2", "Edit B", "Edit", false),
                    service_action("tools", "Tool", "Tools", false),
                ],
                "Edit",
            )
            .iter()
            .map(|row| (row.row_kind, row.name.to_string(), row.action_index))
            .collect::<Vec<_>>(),
            vec![(0, "Edit A".to_string(), 1), (0, "Edit B".to_string(), 2)]
        );
    }

    #[test]
    fn service_menu_policy_rows_keep_disabled_actions_configurable() {
        let actions = vec![
            service_action("top", "Top Action", "", true),
            service_action("tools", "Tool", "Tools", false),
        ];
        let mut policy = ServiceMenuPolicy::default();
        policy.set_enabled("tools", false);

        assert_eq!(
            enabled_actions(&actions, &policy)
                .iter()
                .map(|action| action.id.as_str())
                .collect::<Vec<_>>(),
            vec!["top"]
        );
        assert_eq!(
            policy_action_rows(&actions, &policy)
                .iter()
                .map(|row| (row.id.to_string(), row.name.to_string(), row.enabled))
                .collect::<Vec<_>>(),
            vec![
                ("top".to_string(), "Top Action".to_string(), true),
                ("tools".to_string(), "Tool".to_string(), false),
            ]
        );
    }

    fn service_action(
        id: &str,
        name: &str,
        submenu: &str,
        top_level: bool,
    ) -> service_menu::ServiceMenuAction {
        service_menu::ServiceMenuAction {
            id: id.to_string(),
            name: name.to_string(),
            icon: String::new(),
            desktop_path: PathBuf::from("/tmp/action.desktop"),
            action_key: id.to_string(),
            exec: "true".to_string(),
            argv: vec!["true".to_string()],
            top_level,
            submenu: submenu.to_string(),
        }
    }
}
