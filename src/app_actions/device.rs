use winit::event_loop::ActiveEventLoop;

use super::outcome::{ShellActionEffect, ShellActionOutcome};
use crate::shell::context_menu::ShellContextMenuAction;
use crate::shell::metrics::WGPU_SHELL_PANE_ID;
use crate::shell::tasks::ShellTaskStatus;
use crate::{DeviceActionRequest, FikaWgpuApp};
use fika_core::perform_device_place_operation;

impl FikaWgpuApp {
    pub(crate) fn perform_device_context_action(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        action: ShellContextMenuAction,
    ) {
        let Some(request) = self.scene.context_target_device_action(action) else {
            fika_log!(
                "[fika-wgpu] device-action-error action={} target=none",
                action.as_str()
            );
            self.scene.record_task_status(ShellTaskStatus::failed(
                format!("{} failed", action.label()),
                "No device target",
                false,
            ));
            self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            return;
        };
        let effect = self.perform_device_action_request(request);
        self.apply_action_effect(event_loop, effect);
    }

    pub(crate) fn perform_device_action_request(
        &mut self,
        request: DeviceActionRequest,
    ) -> ShellActionEffect {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return ShellActionOutcome::None.into();
        };
        fika_log!(
            "[fika-wgpu] device-action-start action={} id={:?} label={:?}",
            request.action.as_str(),
            request.id,
            request.label
        );
        let result = pollster::block_on(perform_device_place_operation(
            WGPU_SHELL_PANE_ID,
            request.id.clone(),
            request.label.clone(),
            request.operation,
        ));
        let mount_point = match &result.result {
            Ok(Some(path)) => Some(path.clone()),
            Ok(None) => None,
            Err(error) => {
                fika_log!(
                    "[fika-wgpu] device-action-error action={} id={:?} label={:?} error={error}",
                    request.action.as_str(),
                    request.id,
                    request.label
                );
                None
            }
        };

        match self
            .scene
            .apply_device_place_operation_result(&request, &result, size)
        {
            Ok(()) => {
                if let Some(path) = mount_point {
                    ShellActionEffect::load_path(request.pane, path, "device-mount")
                } else {
                    ShellActionOutcome::Present("device-action").into()
                }
            }
            Err(error) => {
                fika_log!(
                    "[fika-wgpu] device-action-refresh-error action={} id={:?} error={error}",
                    request.action.as_str(),
                    request.id
                );
                ShellActionOutcome::Redraw.into()
            }
        }
    }
}
