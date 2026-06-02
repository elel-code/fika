use super::mime_open;
use crate::fs::privilege;
use crate::{
    AppState, AppWindow, AsyncBridge, AsyncEvent, DesktopApp, FileOpenResult, FileOpenSuccess,
    send_async_event, set_status,
};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Debug)]
pub(crate) struct OpenWithAppsResult {
    pub(crate) path: PathBuf,
    pub(crate) result: Result<(String, Vec<mime_open::CandidateApp>), String>,
}

#[derive(Debug)]
pub(crate) struct OtherApplicationAppsResult {
    pub(crate) path: PathBuf,
    pub(crate) result: Result<(String, Vec<mime_open::CandidateApp>), String>,
}

#[derive(Debug)]
pub(crate) struct DefaultAppSetResult {
    pub(crate) desktop_id: String,
    pub(crate) result: Result<String, String>,
}

pub(crate) fn register_callbacks(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
) {
    {
        let ui_weak = ui.as_weak();
        let bridge = bridge.clone();
        ui.on_prepare_open_with(move |path| {
            if let Some(ui) = ui_weak.upgrade() {
                prepare_open_with_apps(&ui, &bridge, PathBuf::from(path.as_str()));
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let bridge = bridge.clone();
        ui.on_prepare_other_applications(move |path| {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_other_application_filter("".into());
                prepare_other_application_apps(&ui, &bridge, PathBuf::from(path.as_str()));
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(state);
        ui.on_filter_other_applications(move |query| {
            if let Some(ui) = ui_weak.upgrade() {
                filter_other_application_apps(&ui, &state, query.as_str());
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(state);
        let bridge = bridge.clone();
        ui.on_open_with_app(move |path, desktop_id| {
            if let Some(ui) = ui_weak.upgrade() {
                open_file_with_app_async(
                    &ui,
                    &state,
                    &bridge,
                    PathBuf::from(path.as_str()),
                    desktop_id.to_string(),
                );
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let bridge = bridge.clone();
        ui.on_set_default_app(move |path, desktop_id| {
            if let Some(ui) = ui_weak.upgrade() {
                set_default_app_async(
                    &ui,
                    &bridge,
                    PathBuf::from(path.as_str()),
                    desktop_id.to_string(),
                );
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(state);
        let bridge = bridge.clone();
        ui.on_open_with_custom_command(move |path, command| {
            if let Some(ui) = ui_weak.upgrade() {
                open_file_with_custom_command_async(
                    &ui,
                    &state,
                    &bridge,
                    PathBuf::from(path.as_str()),
                    command.to_string(),
                );
            }
        });
    }
}

fn open_file_with_app_async(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    path: PathBuf,
    desktop_id: String,
) {
    let generation = {
        let mut state = state.borrow_mut();
        state.panes.active.open_generation.next()
    };
    set_status(ui, &format!("Opening with {desktop_id}..."));

    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let result = open_with_app_with_privilege_fallback(path.clone(), desktop_id.clone()).await;
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::FileOpened(FileOpenResult {
                generation,
                path,
                result,
            }),
        );
    });
}

fn open_file_with_custom_command_async(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    bridge: &AsyncBridge,
    path: PathBuf,
    command: String,
) {
    let generation = {
        let mut state = state.borrow_mut();
        state.panes.active.open_generation.next()
    };
    set_status(ui, "Opening with custom command...");

    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let result =
            open_with_custom_command_with_privilege_fallback(path.clone(), command.clone()).await;
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::FileOpened(FileOpenResult {
                generation,
                path,
                result,
            }),
        );
    });
}

async fn open_with_app_with_privilege_fallback(
    path: PathBuf,
    desktop_id: String,
) -> Result<FileOpenSuccess, String> {
    let open_path = path.clone();
    let app = desktop_id.clone();
    let direct =
        match tokio::task::spawn_blocking(move || mime_open::open_file_with_app(&open_path, &app))
            .await
        {
            Ok(result) => result,
            Err(err) => return Err(format!("file open task failed: {err}")),
        };

    match direct {
        Ok(mime_type) => Ok(FileOpenSuccess {
            mime_type: mime_type.mime_type,
            unit: mime_type.unit,
            launch_diagnostic: mime_type.launch_diagnostic,
            external_edit: None,
        }),
        Err(err) if privilege::is_permission_error(&err) => {
            let mut session = privilege::prepare_external_edit_via_dbus(path).await?;
            let scratch_path = session.scratch_path.clone();
            let app = desktop_id.clone();
            let launch = match tokio::task::spawn_blocking(move || {
                mime_open::open_file_with_app(&scratch_path, &app)
            })
            .await
            {
                Ok(result) => result?,
                Err(err) => return Err(format!("file open task failed: {err}")),
            };
            session.unit = launch.unit.clone();
            if let Err(err) = privilege::associate_external_edit_unit_via_dbus(&session).await {
                eprintln!("[fika launch] cannot associate protected edit with systemd unit: {err}");
            }
            Ok(FileOpenSuccess {
                mime_type: launch.mime_type,
                unit: launch.unit,
                launch_diagnostic: launch.launch_diagnostic,
                external_edit: Some(session),
            })
        }
        Err(err) => Err(err),
    }
}

async fn open_with_custom_command_with_privilege_fallback(
    path: PathBuf,
    command: String,
) -> Result<FileOpenSuccess, String> {
    let open_path = path.clone();
    let command_for_direct = command.clone();
    let direct = match tokio::task::spawn_blocking(move || {
        mime_open::open_file_with_custom_command(&open_path, &command_for_direct)
    })
    .await
    {
        Ok(result) => result,
        Err(err) => return Err(format!("file open task failed: {err}")),
    };

    match direct {
        Ok(launch) => Ok(FileOpenSuccess {
            mime_type: "custom command".to_string(),
            unit: launch.unit,
            launch_diagnostic: launch.diagnostic,
            external_edit: None,
        }),
        Err(err) if privilege::is_permission_error(&err) => {
            let mut session = privilege::prepare_external_edit_via_dbus(path).await?;
            let scratch_path = session.scratch_path.clone();
            let command_for_scratch = command.clone();
            let launch = match tokio::task::spawn_blocking(move || {
                mime_open::open_file_with_custom_command(&scratch_path, &command_for_scratch)
            })
            .await
            {
                Ok(result) => result?,
                Err(err) => return Err(format!("file open task failed: {err}")),
            };
            session.unit = launch.unit.clone();
            if let Err(err) = privilege::associate_external_edit_unit_via_dbus(&session).await {
                eprintln!("[fika launch] cannot associate protected edit with systemd unit: {err}");
            }
            Ok(FileOpenSuccess {
                mime_type: "custom command".to_string(),
                unit: launch.unit,
                launch_diagnostic: launch.diagnostic,
                external_edit: Some(session),
            })
        }
        Err(err) => Err(err),
    }
}

fn prepare_open_with_apps(ui: &AppWindow, bridge: &AsyncBridge, path: PathBuf) {
    ui.set_open_with_path(path.display().to_string().into());
    ui.set_open_with_mime("Loading...".into());
    ui.set_default_open_app_name("".into());
    ui.set_open_with_apps(ModelRc::new(Rc::new(VecModel::from(
        Vec::<DesktopApp>::new(),
    ))));

    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let query_path = path.clone();
        let result =
            match tokio::task::spawn_blocking(move || mime_open::list_apps_for_file(&query_path))
                .await
            {
                Ok(result) => result,
                Err(err) => Err(format!("open-with query task failed: {err}")),
            };
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::OpenWithAppsLoaded(OpenWithAppsResult { path, result }),
        );
    });
}

fn prepare_other_application_apps(ui: &AppWindow, bridge: &AsyncBridge, path: PathBuf) {
    ui.set_open_with_path(path.display().to_string().into());
    ui.set_other_application_apps(ModelRc::new(Rc::new(VecModel::from(
        Vec::<DesktopApp>::new(),
    ))));

    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let query_path = path.clone();
        let result = match tokio::task::spawn_blocking(move || {
            mime_open::list_other_apps_for_file(&query_path)
        })
        .await
        {
            Ok(result) => result,
            Err(err) => Err(format!("other-applications query task failed: {err}")),
        };
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::OtherApplicationAppsLoaded(OtherApplicationAppsResult { path, result }),
        );
    });
}

fn set_default_app_async(ui: &AppWindow, bridge: &AsyncBridge, path: PathBuf, desktop_id: String) {
    set_status(ui, &format!("Setting default app to {desktop_id}..."));

    let async_tx = bridge.tx.clone();
    let notify_ui = bridge.ui_weak.clone();
    bridge.handle.spawn(async move {
        let set_path = path.clone();
        let app = desktop_id.clone();
        let result = match tokio::task::spawn_blocking(move || {
            mime_open::set_default_app_for_file(&set_path, &app)
        })
        .await
        {
            Ok(result) => result,
            Err(err) => Err(format!("set default app task failed: {err}")),
        };
        send_async_event(
            async_tx,
            notify_ui,
            AsyncEvent::DefaultAppSet(DefaultAppSetResult { desktop_id, result }),
        );
    });
}

pub(crate) fn apply_open_with_apps_result(ui: &AppWindow, result: OpenWithAppsResult) {
    if ui.get_open_with_path().as_str() != result.path.to_string_lossy().as_ref() {
        return;
    }

    match result.result {
        Ok((mime_type, apps)) => {
            let default_name = apps
                .iter()
                .find(|app| app.is_default)
                .map(|app| app.name.as_str())
                .unwrap_or("");
            ui.set_open_with_mime(mime_type.into());
            ui.set_default_open_app_name(default_name.into());
            ui.set_open_with_apps(ModelRc::new(Rc::new(VecModel::from(to_desktop_apps(apps)))));
        }
        Err(err) => {
            ui.set_open_with_mime("Open With".into());
            ui.set_default_open_app_name("".into());
            ui.set_open_with_apps(ModelRc::new(Rc::new(VecModel::from(
                Vec::<DesktopApp>::new(),
            ))));
            set_status(ui, &format!("Cannot list applications: {err}"));
        }
    }
}

pub(crate) fn apply_other_application_apps_result(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    result: OtherApplicationAppsResult,
) {
    if ui.get_open_with_path().as_str() != result.path.to_string_lossy().as_ref() {
        return;
    }

    match result.result {
        Ok((mime_type, apps)) => {
            ui.set_open_with_mime(mime_type.into());
            let apps = to_desktop_apps(apps);
            state.borrow_mut().other_application_apps = apps.clone();
            ui.set_other_application_apps(ModelRc::new(Rc::new(VecModel::from(apps))));
        }
        Err(err) => {
            state.borrow_mut().other_application_apps.clear();
            ui.set_other_application_apps(ModelRc::new(Rc::new(VecModel::from(
                Vec::<DesktopApp>::new(),
            ))));
            set_status(ui, &format!("Cannot list applications: {err}"));
        }
    }
}

fn filter_other_application_apps(ui: &AppWindow, state: &Rc<RefCell<AppState>>, query: &str) {
    let query = query.trim().to_ascii_lowercase();
    let apps = state
        .borrow()
        .other_application_apps
        .iter()
        .filter(|app| {
            query.is_empty()
                || app.name.to_ascii_lowercase().contains(&query)
                || app.id.to_ascii_lowercase().contains(&query)
        })
        .cloned()
        .collect::<Vec<_>>();
    ui.set_other_application_apps(ModelRc::new(Rc::new(VecModel::from(apps))));
}

pub(crate) fn apply_default_app_set_result(ui: &AppWindow, result: DefaultAppSetResult) {
    match result.result {
        Ok(mime_type) => {
            set_status(
                ui,
                &format!("Default app for {mime_type} set to {}", result.desktop_id),
            );
        }
        Err(err) => set_status(ui, &format!("Cannot set default app: {err}")),
    }
}

fn to_desktop_apps(apps: Vec<mime_open::CandidateApp>) -> Vec<DesktopApp> {
    apps.into_iter()
        .map(|app| DesktopApp {
            id: app.desktop_id.into(),
            name: app.name.into(),
            is_default: app.is_default,
        })
        .collect()
}
