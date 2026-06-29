use std::path::Path;

use winit::event_loop::ActiveEventLoop;

use super::outcome::ShellActionOutcome;
use crate::shell::create_rename::disk::{
    create_entry_on_disk_explicit, rename_entry_on_disk_explicit,
};
use crate::shell::privilege::should_attempt_privileged_operation;
use crate::shell::tasks::ShellTaskStatus;
use crate::{FikaWgpuApp, path_display_label, task_error_detail};
use fika_core::{MimeApplicationCache, set_default_mime_application};

impl FikaWgpuApp {
    pub(crate) fn commit_rename_dialog(&mut self, event_loop: &dyn ActiveEventLoop) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let request = match self.scene.rename_entry_request() {
            Ok(request) => request,
            Err(error) => {
                if self.scene.set_rename_dialog_error(error) {
                    self.finish_rename_dialog_state_change();
                }
                return;
            }
        };

        let outcome = match rename_entry_on_disk_explicit(&request) {
            Ok(outcome) => outcome,
            Err(error) => {
                let administrator_available =
                    !request.privileged && should_attempt_privileged_operation(&error);
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Rename failed",
                    task_error_detail(&error, administrator_available),
                    request.privileged,
                ));
                if self.scene.set_rename_dialog_error(error) {
                    self.finish_rename_dialog_state_change();
                }
                return;
            }
        };

        if outcome.privileged {
            self.scene.record_task_status(ShellTaskStatus::completed(
                "Administrator rename",
                format!(
                    "{} to {}",
                    path_display_label(&request.source),
                    outcome
                        .message
                        .clone()
                        .unwrap_or_else(|| request.name.clone())
                ),
                true,
            ));
        } else {
            self.scene.record_task_status(ShellTaskStatus::completed(
                "Renamed",
                format!(
                    "{} to {}",
                    request.original_name,
                    path_display_label(&request.target)
                ),
                false,
            ));
        }

        let affected_dir = request.target.parent().map(Path::to_path_buf);
        self.scene.close_rename_dialog_after_success(&request);
        self.close_rename_dialog_window();
        let reload_result = affected_dir
            .as_deref()
            .ok_or_else(|| format!("rename target has no parent: {}", request.target.display()))
            .and_then(|dir| self.scene.reload_panes_showing_path(dir, size));
        match reload_result {
            Ok(_) => {
                self.scene
                    .select_entry_by_name_in_pane(request.pane, &request.name, size);
                self.apply_action_outcome(event_loop, ShellActionOutcome::Present("rename"));
            }
            Err(error) => {
                fika_log!("[fika-wgpu] rename-reload-error {error}");
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            }
        }
    }

    pub(crate) fn commit_create_dialog(&mut self, event_loop: &dyn ActiveEventLoop) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let request = match self.scene.create_entry_request() {
            Ok(request) => request,
            Err(error) => {
                if self.scene.set_create_dialog_error(error) {
                    self.finish_create_dialog_state_change();
                }
                return;
            }
        };

        let outcome = match create_entry_on_disk_explicit(&request) {
            Ok(outcome) => outcome,
            Err(error) => {
                let administrator_available =
                    !request.privileged && should_attempt_privileged_operation(&error);
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Create failed",
                    task_error_detail(&error, administrator_available),
                    request.privileged,
                ));
                if self.scene.set_create_dialog_error(error) {
                    self.finish_create_dialog_state_change();
                }
                return;
            }
        };

        if outcome.privileged {
            self.scene.record_task_status(ShellTaskStatus::completed(
                format!("Administrator create {}", request.kind.as_str()),
                format!(
                    "{} in {}",
                    outcome
                        .message
                        .clone()
                        .unwrap_or_else(|| request.name.clone()),
                    request.parent.display()
                ),
                true,
            ));
        } else {
            self.scene.record_task_status(ShellTaskStatus::completed(
                format!("Created {}", request.kind.as_str()),
                format!("{} in {}", request.name, request.parent.display()),
                false,
            ));
        }

        self.scene.close_create_dialog_after_success(&request);
        self.close_create_dialog_window();
        match self.scene.reload_panes_showing_path(&request.parent, size) {
            Ok(_) => {
                self.scene
                    .select_entry_by_name_in_pane(request.pane, &request.name, size);
                self.apply_action_outcome(event_loop, ShellActionOutcome::Present("create-new"));
            }
            Err(error) => {
                fika_log!("[fika-wgpu] create-new-reload-error {error}");
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            }
        }
    }

    pub(crate) fn commit_open_with_chooser(&mut self) {
        let request = match self.scene.open_with_launch_request(&self.mime_applications) {
            Ok(request) => request,
            Err(error) => {
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Open With failed",
                    error.clone(),
                    false,
                ));
                if self.scene.set_open_with_chooser_error(error) {
                    self.finish_open_with_dialog_state_change();
                }
                return;
            }
        };

        if let Some(default_update) = request.default_update.as_ref() {
            match set_default_mime_application(
                &default_update.mime_type,
                &default_update.desktop_id,
            ) {
                Ok(path) => {
                    fika_log!(
                        "[fika-wgpu] open-with-default mime={} desktop={} path={}",
                        default_update.mime_type,
                        default_update.desktop_id,
                        path.display()
                    );
                    self.mime_applications = MimeApplicationCache::load();
                }
                Err(error) => {
                    self.scene.record_task_status(ShellTaskStatus::failed(
                        "Set Default Application failed",
                        error.clone(),
                        false,
                    ));
                    if self.scene.set_open_with_chooser_error(error) {
                        self.finish_open_with_dialog_state_change();
                    }
                    return;
                }
            }
        }

        self.scene.close_open_with_chooser_after_success(&request);
        self.close_open_with_dialog_window();
        self.launch_open_with_request(request);
    }
}
