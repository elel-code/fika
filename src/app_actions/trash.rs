use crate::platform::ActiveEventLoop;

use super::outcome::ShellActionOutcome;
use crate::FikaWgpuApp;
use crate::shell::context_menu::ShellContextMenuAction;
use crate::shell::tasks::ShellTaskStatus;

impl FikaWgpuApp {
    pub(crate) fn perform_trash_view_context_action(
        &mut self,
        event_loop: &ActiveEventLoop,
        action: ShellContextMenuAction,
    ) {
        if action == ShellContextMenuAction::EmptyTrash {
            match self.start_async_trash_view_operation(action) {
                Ok(()) => self.apply_window_action_outcome(ShellActionOutcome::Redraw),
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
                    self.apply_window_action_outcome(ShellActionOutcome::Redraw);
                }
            }
            return;
        }

        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        match self.scene.perform_trash_view_context_action(action, size) {
            Ok(result) if result.success_count > 0 => {
                self.apply_action_outcome(event_loop, ShellActionOutcome::Present(action.as_str()));
            }
            Ok(_) => self.apply_window_action_outcome(ShellActionOutcome::Redraw),
            Err(error) => {
                fika_log!(
                    "[fika-wgpu] trash-view-error action={} {error}",
                    action.as_str()
                );
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            }
        }
    }

    pub(crate) fn move_context_target_to_trash(
        &mut self,
        event_loop: &ActiveEventLoop,
        privileged: bool,
    ) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        match self.scene.move_context_target_to_trash(size, privileged) {
            Ok(result) if result.changed() => {
                self.apply_action_outcome(event_loop, ShellActionOutcome::Present("move-to-trash"));
            }
            Ok(_) => self.apply_window_action_outcome(ShellActionOutcome::Redraw),
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
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            }
        }
    }

    pub(crate) fn replace_trash_restore_conflicts(&mut self, event_loop: &ActiveEventLoop) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        match self.scene.replace_trash_restore_conflicts(size) {
            Ok(result) if result.success_count > 0 => {
                self.apply_action_outcome(
                    event_loop,
                    ShellActionOutcome::Present("replace-trash-conflicts"),
                );
            }
            Ok(_) => self.apply_window_action_outcome(ShellActionOutcome::Redraw),
            Err(error) => {
                fika_log!("[fika-wgpu] trash-conflict-error {error}");
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            }
        }
    }
}
