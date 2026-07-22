use crate::platform::MouseScrollDelta;

use super::outcome::ShellActionOutcome;
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
                self.apply_window_action_outcome(ShellActionOutcome::Queue {
                    reason: "wheel-zoom",
                    redraw_frames: ZOOM_REDRAW_FRAMES,
                });
            }
            return;
        }
        if self.scene.scroll_by(delta_y, size) {
            self.apply_window_action_outcome(ShellActionOutcome::Queue {
                reason: "wheel-scroll",
                redraw_frames: SCROLL_REDRAW_FRAMES,
            });
        }
    }
}
