use winit::event_loop::ActiveEventLoop;

use crate::FikaWgpuApp;
use crate::shell::context_menu::ShellContextMenuAction;
use crate::shell::tasks::ShellTaskStatus;

impl FikaWgpuApp {
    pub(crate) fn perform_trash_view_context_action(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        action: ShellContextMenuAction,
    ) {
        if action == ShellContextMenuAction::EmptyTrash {
            match self.start_async_trash_view_operation(action) {
                Ok(()) => {
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                }
                Err(error) => {
                    fika_log!(
                        "[fika-wgpu] trash-view-error action={} {error}",
                        action.as_str()
                    );
                    self.scene.record_task_status(ShellTaskStatus::failed(
                        "Empty Trash failed",
                        error,
                        false,
                    ));
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                }
            }
            return;
        }

        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        match self.scene.perform_trash_view_context_action(action, size) {
            Ok(result) if result.success_count > 0 => {
                self.present_scene_change(event_loop, action.as_str())
            }
            Ok(_) => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            Err(error) => {
                fika_log!(
                    "[fika-wgpu] trash-view-error action={} {error}",
                    action.as_str()
                );
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
        }
    }

    pub(crate) fn move_context_target_to_trash(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        privileged: bool,
    ) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        match self.scene.move_context_target_to_trash(size, privileged) {
            Ok(result) if result.changed() => {
                self.present_scene_change(event_loop, "move-to-trash")
            }
            Ok(_) => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            Err(error) => {
                fika_log!("[fika-wgpu] trash-error {error}");
                self.scene.record_task_status(ShellTaskStatus::failed(
                    if privileged {
                        "Administrator move to Trash failed"
                    } else {
                        "Move to Trash failed"
                    },
                    error,
                    privileged,
                ));
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
        }
    }
}
