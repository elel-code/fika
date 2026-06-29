use winit::event_loop::ActiveEventLoop;

use crate::FikaWgpuApp;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShellActionOutcome {
    None,
    Redraw,
    Queue {
        reason: &'static str,
        redraw_frames: u8,
    },
    Present(&'static str),
}

impl ShellActionOutcome {
    pub(crate) fn redraw_if(changed: bool) -> Self {
        if changed { Self::Redraw } else { Self::None }
    }

    pub(crate) fn present_if(changed: bool, reason: &'static str) -> Self {
        if changed {
            Self::Present(reason)
        } else {
            Self::None
        }
    }
}

impl FikaWgpuApp {
    pub(crate) fn apply_action_outcome(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        outcome: ShellActionOutcome,
    ) {
        match outcome {
            ShellActionOutcome::Present(reason) => self.present_scene_change(event_loop, reason),
            outcome => self.apply_window_action_outcome(outcome),
        }
    }

    pub(crate) fn apply_window_action_outcome(&mut self, outcome: ShellActionOutcome) {
        match outcome {
            ShellActionOutcome::None => {}
            ShellActionOutcome::Redraw => self.request_main_redraw(),
            ShellActionOutcome::Queue {
                reason,
                redraw_frames,
            } => self.queue_scene_change(reason, redraw_frames),
            ShellActionOutcome::Present(reason) => {
                fika_log!("[fika-wgpu] action-outcome-present-without-event-loop reason={reason}");
                self.request_main_redraw();
            }
        }
    }
}
