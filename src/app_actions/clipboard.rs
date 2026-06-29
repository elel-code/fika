use crate::shell::clipboard::FileClipboardExportRequest;
use crate::shell::tasks::ShellTaskStatus;
use crate::{FikaWgpuApp, file_clipboard_role_as_str, paths_task_summary};

impl FikaWgpuApp {
    pub(crate) fn store_file_clipboard_request(&mut self, request: &FileClipboardExportRequest) {
        if let Some(clipboard) = self.clipboard.as_ref() {
            match clipboard.store_text(&request.text) {
                Ok(()) => self.scene.record_file_clipboard_export(request),
                Err(error) => {
                    fika_log!(
                        "[fika-wgpu] clipboard-export-error role={} paths={} error={error}",
                        file_clipboard_role_as_str(request.role),
                        request.paths.len()
                    );
                    self.scene.record_task_status(ShellTaskStatus::failed(
                        "Clipboard failed",
                        format!(
                            "Could not store {}: {error}",
                            paths_task_summary(&request.paths)
                        ),
                        false,
                    ));
                }
            }
        } else {
            fika_log!(
                "[fika-wgpu] clipboard-export-error role={} paths={} error=clipboard-unavailable",
                file_clipboard_role_as_str(request.role),
                request.paths.len()
            );
            self.scene.record_task_status(ShellTaskStatus::failed(
                "Clipboard failed",
                format!(
                    "Clipboard is unavailable for {}",
                    paths_task_summary(&request.paths)
                ),
                false,
            ));
        }
    }
}
