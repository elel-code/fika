use winit::event_loop::ActiveEventLoop;

use super::outcome::ShellActionOutcome;
use crate::FikaWgpuApp;
use crate::shell::drop_menu::ShellDropOperationRequest;
use crate::shell::tasks::ShellTaskStatus;
use crate::shell::transfer::ShellAsyncTransferSource;
use fika_core::{FileClipboardRole, FileTransferMode, decode_file_clipboard_text, is_network_path};

impl FikaWgpuApp {
    pub(crate) fn paste_from_clipboard(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        privileged: bool,
    ) {
        self.paste_from_clipboard_with_target(event_loop, true, privileged);
    }

    pub(crate) fn paste_from_clipboard_into_active_pane(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
    ) {
        self.paste_from_clipboard_with_target(event_loop, false, false);
    }

    pub(crate) fn paste_from_clipboard_with_target(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        use_context: bool,
        privileged: bool,
    ) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let Some(clipboard) = self.clipboard.as_ref() else {
            fika_log!("[fika-wgpu] paste-error error=clipboard-unavailable");
            self.scene.record_task_status(ShellTaskStatus::failed(
                "Paste failed",
                "Clipboard is unavailable",
                false,
            ));
            self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            return;
        };
        let text = match clipboard.load_text() {
            Ok(text) => text,
            Err(error) => {
                fika_log!("[fika-wgpu] paste-error load={error}");
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Paste failed",
                    format!("Clipboard read failed: {error}"),
                    false,
                ));
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
                return;
            }
        };
        if !privileged && let Some(payload) = decode_file_clipboard_text(&text) {
            let target_dir = if use_context {
                self.scene
                    .context_target_paste_directory()
                    .or_else(|| self.scene.active_pane_paste_directory())
            } else {
                self.scene.active_pane_paste_directory()
            };
            let Some((_target_pane, target_dir)) = target_dir else {
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Paste failed",
                    "No paste target pane",
                    false,
                ));
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
                return;
            };
            if is_network_path(&target_dir) {
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Paste failed",
                    "Remote paste target is not available yet",
                    false,
                ));
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
                return;
            }
            if payload.paths.iter().any(|path| is_network_path(path)) {
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Paste failed",
                    "Remote paste source is not available yet",
                    false,
                ));
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
                return;
            }
            let mode = match payload.role {
                FileClipboardRole::Copy => FileTransferMode::Copy,
                FileClipboardRole::Cut => FileTransferMode::Move,
            };
            self.start_async_transfer(
                ShellAsyncTransferSource::Paste,
                target_dir,
                mode,
                payload.paths,
                "Paste",
                payload.role == FileClipboardRole::Cut,
            );
            self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            return;
        }
        let paste_result = if use_context {
            self.scene
                .paste_clipboard_text_from_context(&text, size, privileged)
        } else {
            self.scene
                .paste_clipboard_text_into_active_pane(&text, size, privileged)
        };
        match paste_result {
            Ok(result) if result.changed() => {
                if result.clear_clipboard
                    && result.failure_count == 0
                    && let Err(error) = clipboard.store_text("")
                {
                    fika_log!("[fika-wgpu] clipboard-clear-error error={error}");
                }
                self.apply_action_outcome(event_loop, ShellActionOutcome::Present("paste"));
            }
            Ok(_) => self.apply_window_action_outcome(ShellActionOutcome::Redraw),
            Err(error) => {
                fika_log!("[fika-wgpu] paste-error {error}");
                self.scene.record_task_status(ShellTaskStatus::failed(
                    if privileged {
                        "Administrator paste failed"
                    } else {
                        "Paste failed"
                    },
                    error,
                    privileged,
                ));
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            }
        }
    }

    pub(crate) fn perform_drop_operation_request(
        &mut self,
        request: ShellDropOperationRequest,
    ) -> ShellActionOutcome {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return ShellActionOutcome::None;
        };
        if !request.privileged {
            if let Err(error) = self.scene.validate_drop_operation_request(&request) {
                self.scene
                    .record_task_status(ShellTaskStatus::failed("Drop failed", error, false));
                return ShellActionOutcome::Redraw;
            }
            self.start_async_transfer(
                ShellAsyncTransferSource::Drop,
                request.target_dir,
                request.mode,
                request.sources,
                request.mode.label(),
                false,
            );
            return ShellActionOutcome::Redraw;
        }
        match self.scene.perform_drop_operation_request(&request, size) {
            Ok(result) if result.changed() => ShellActionOutcome::Present("dnd-drop"),
            Ok(_) => ShellActionOutcome::Redraw,
            Err(error) => {
                fika_log!("[fika-wgpu] dnd-transfer-error {error}");
                self.scene.record_task_status(ShellTaskStatus::failed(
                    if request.privileged {
                        "Administrator drop failed"
                    } else {
                        "Drop failed"
                    },
                    error,
                    request.privileged,
                ));
                ShellActionOutcome::Redraw
            }
        }
    }
}
