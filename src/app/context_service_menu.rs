use crate::app::async_bridge::{AsyncBridge, send_async_event};
use crate::app::events::{AsyncEvent, ServiceMenuActionLaunchResult};
use crate::app::split_view::sync_pane_slot_ui;
use crate::app::state::AppState;
use crate::desktop::service_menu;
use crate::{AppWindow, ContextServiceAction};
use slint::{ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

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
        state.context_service_menu_pane_id = state.panes.pane_for_slot(slot).map(|pane| pane.id);
        generation
    };
    sync_actions_ui(ui, &[]);

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
    let actions = {
        let mut state = state.borrow_mut();
        if !state.service_menu_generation.is_current(result.generation)
            || state.context_service_menu_paths != result.paths
        {
            return;
        }
        state.context_service_menu_actions = result.result.unwrap_or_default();
        state.context_service_menu_actions.clone()
    };
    sync_actions_ui(ui, &actions);
}

fn sync_actions_ui(ui: &AppWindow, actions: &[service_menu::ServiceMenuAction]) {
    let actions = actions
        .iter()
        .map(|action| ContextServiceAction {
            id: action.id.clone().into(),
            name: action.name.clone().into(),
        })
        .collect::<Vec<_>>();
    ui.set_context_service_actions(ModelRc::new(Rc::new(VecModel::from(actions))));
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
            ) && refresh_body.contains("sync_actions_ui(ui, &[]);"),
            "opening a context menu should remember the source pane and clear stale service action rows before async discovery"
        );
        assert!(
            apply_body.contains("let actions = {\n        let mut state = state.borrow_mut();")
                && apply_body.contains("state.context_service_menu_actions.clone()")
                && apply_body.contains("sync_actions_ui(ui, &actions);"),
            "service menu discovery results should update AppState first, release the borrow, then write the Slint model"
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
}
