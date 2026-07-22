use crate::platform::ActiveEventLoop;

use super::outcome::ShellActionOutcome;
use crate::FikaWgpuApp;
use crate::shell::drop_menu::ShellDropOperationRequest;
use crate::shell::tasks::ShellTaskStatus;
use crate::shell::transfer::ShellAsyncTransferSource;
use fika_core::{FileClipboardRole, FileTransferMode, decode_file_clipboard_text, is_network_path};

impl FikaWgpuApp {
    pub(crate) fn paste_from_clipboard(&mut self, event_loop: &ActiveEventLoop, privileged: bool) {
        self.paste_from_clipboard_with_target(event_loop, true, privileged);
    }

    pub(crate) fn paste_from_clipboard_into_active_pane(&mut self, event_loop: &ActiveEventLoop) {
        self.paste_from_clipboard_with_target(event_loop, false, false);
    }

    pub(crate) fn paste_from_clipboard_with_target(
        &mut self,
        _event_loop: &ActiveEventLoop,
        use_context: bool,
        privileged: bool,
    ) {
        if self.renderer.is_none() {
            return;
        }
        self.load_clipboard_text_for_paste(use_context, privileged);
        self.apply_window_action_outcome(ShellActionOutcome::Redraw);
    }

    pub(crate) fn finish_paste_from_clipboard_text(
        &mut self,
        use_context: bool,
        privileged: bool,
        text: String,
    ) -> bool {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return false;
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
                return true;
            };
            if is_network_path(&target_dir) {
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Paste failed",
                    "Remote paste target is not available yet",
                    false,
                ));
                return true;
            }
            if payload.paths.iter().any(|path| is_network_path(path)) {
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Paste failed",
                    "Remote paste source is not available yet",
                    false,
                ));
                return true;
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
            return true;
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
                if result.clear_clipboard && result.failure_count == 0 {
                    self.queue_clipboard_clear("paste-text");
                }
                true
            }
            Ok(_) => true,
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
                true
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
