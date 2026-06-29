use winit::cursor::CursorIcon;
use winit::dpi::PhysicalPosition;

use super::outcome::ShellActionOutcome;
use crate::{FikaWgpuApp, view_point_from_physical_position};

impl FikaWgpuApp {
    pub(crate) fn handle_main_pointer_moved(&mut self, position: PhysicalPosition<f64>) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let point = view_point_from_physical_position(position);
        if self.scene.is_task_detail_dialog_open() {
            self.set_window_cursor(CursorIcon::Default);
            return;
        }
        let changed = self.scene.set_pointer(point, size);
        self.update_window_cursor_for_scene(size);
        self.apply_window_action_outcome(ShellActionOutcome::redraw_if(changed));
    }

    pub(crate) fn handle_main_pointer_left(&mut self) {
        self.set_window_cursor(CursorIcon::Default);
        let changed = self.scene.clear_pointer();
        self.apply_window_action_outcome(ShellActionOutcome::redraw_if(changed));
    }
}
