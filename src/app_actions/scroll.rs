use winit::event::MouseScrollDelta;

use crate::shell::shortcuts::zoom_action_for_scroll_delta;
use crate::{FikaWgpuApp, SCROLL_REDRAW_FRAMES, ZOOM_REDRAW_FRAMES, scroll_delta_y};

impl FikaWgpuApp {
    pub(crate) fn handle_main_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let delta_y = scroll_delta_y(delta, self.scene.ui_scale());
        let shortcut = self.modifiers.state().control_key() || self.modifiers.state().meta_key();
        if shortcut {
            if let Some(zoom_action) = zoom_action_for_scroll_delta(delta_y)
                && self.scene.zoom(zoom_action, size)
            {
                self.queue_scene_change("wheel-zoom", ZOOM_REDRAW_FRAMES);
            }
            return;
        }
        if self.scene.scroll_by(delta_y, size) {
            self.queue_scene_change("wheel-scroll", SCROLL_REDRAW_FRAMES);
        }
    }
}
