use winit::dpi::PhysicalSize;
use winit::event_loop::ActiveEventLoop;

use crate::FikaWgpuApp;
use crate::shell::location::LocationDraftPurpose;
use crate::shell::shortcuts::PathNavigationAction;
use crate::shell::tasks::ShellTaskStatus;
use fika_core::default_user_places_path;

impl FikaWgpuApp {
    pub(crate) fn perform_path_navigation(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        action: PathNavigationAction,
    ) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let result = match action {
            PathNavigationAction::Back => self.scene.go_history_back(size),
            PathNavigationAction::Forward => self.scene.go_history_forward(size),
            PathNavigationAction::Parent => self.scene.go_parent_directory(size),
        };
        match result {
            Ok(true) => self.present_scene_change(event_loop, action.reason()),
            Ok(false) => {}
            Err(error) => fika_log!("[fika-wgpu] navigation-error {error}"),
        }
    }

    pub(crate) fn reload_scene_path(&mut self, event_loop: &dyn ActiveEventLoop) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        match self.scene.reload_current_path(size) {
            Ok(true) => {
                self.scene.record_task_status(ShellTaskStatus::completed(
                    "Refreshed",
                    self.scene.active_pane_path_label(),
                    false,
                ));
                self.present_scene_change(event_loop, "reload-directory");
            }
            Ok(false) => {
                self.scene.record_task_status(ShellTaskStatus::completed(
                    "Refresh skipped",
                    self.scene.active_pane_path_label(),
                    false,
                ));
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            Err(error) => {
                fika_log!("[fika-wgpu] reload-error {error}");
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Refresh failed",
                    error,
                    false,
                ));
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
        }
    }

    pub(crate) fn commit_location_draft(&mut self, event_loop: &dyn ActiveEventLoop) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        if self.scene.location_draft_purpose() == Some(LocationDraftPurpose::AddNetworkFolder) {
            self.commit_add_network_folder_draft(event_loop, size);
            return;
        }
        let input = self.scene.location_draft_value().unwrap_or("").to_string();
        let Some((pane, path)) = self.scene.resolved_location_draft() else {
            fika_log!("[fika-wgpu] location-error input={input:?} error=empty");
            return;
        };
        let closed = self.scene.close_location_draft(size);
        match self.scene.load_path_for_pane(pane, path, size) {
            Ok(true) => self.present_scene_change(event_loop, "location-commit"),
            Ok(false) => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            Err(error) => {
                fika_log!("[fika-wgpu] location-error input={input:?} error={error}");
                if closed && let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
        }
    }

    fn commit_add_network_folder_draft(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        size: PhysicalSize<u32>,
    ) {
        let input = self.scene.location_draft_value().unwrap_or("").to_string();
        let request = match self.scene.add_network_folder_request_from_draft() {
            Ok(request) => request,
            Err(error) => {
                fika_log!("[fika-wgpu] add-network-folder-error input={input:?} error={error}");
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Add Network Folder failed",
                    error,
                    false,
                ));
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                return;
            }
        };

        let _ = self.scene.close_location_draft(size);
        match self.scene.add_network_folder_place(
            &default_user_places_path(),
            &request.path,
            request.label,
            size,
        ) {
            Ok(_) => match self
                .scene
                .load_path_for_pane(request.pane, request.path, size)
            {
                Ok(true) => self.present_scene_change(event_loop, "add-network-folder"),
                Ok(false) => {
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                }
                Err(error) => {
                    fika_log!("[fika-wgpu] add-network-folder-load-error {error}");
                    self.scene.record_task_status(ShellTaskStatus::failed(
                        "Open Network Folder failed",
                        error,
                        false,
                    ));
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                }
            },
            Err(error) => {
                fika_log!("[fika-wgpu] add-network-folder-error input={input:?} error={error}");
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Add Network Folder failed",
                    error,
                    false,
                ));
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
        }
    }
}
