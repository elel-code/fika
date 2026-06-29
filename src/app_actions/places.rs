use winit::event_loop::ActiveEventLoop;

use crate::FikaWgpuApp;
use crate::shell::tasks::ShellTaskStatus;
use fika_core::default_user_places_path;

impl FikaWgpuApp {
    pub(crate) fn open_add_network_folder_input(&mut self) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        if self.scene.open_add_network_folder_location_draft(size)
            && let Some(window) = self.window.as_ref()
        {
            window.request_redraw();
        }
    }

    pub(crate) fn add_context_target_to_places(&mut self, event_loop: &dyn ActiveEventLoop) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        match self
            .scene
            .add_context_target_to_places(&default_user_places_path(), size)
        {
            Ok(true) => self.present_scene_change(event_loop, "add-place"),
            Ok(false) => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            Err(error) => {
                fika_log!("[fika-wgpu] add-place-error {error}");
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Add to Places failed",
                    error,
                    false,
                ));
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
        }
    }

    pub(crate) fn remove_context_place(&mut self, event_loop: &dyn ActiveEventLoop) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        match self
            .scene
            .remove_context_place(&default_user_places_path(), size)
        {
            Ok(true) => self.present_scene_change(event_loop, "remove-place"),
            Ok(false) => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            Err(error) => {
                fika_log!("[fika-wgpu] remove-place-error {error}");
                self.scene.record_task_status(ShellTaskStatus::failed(
                    "Remove Place failed",
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
