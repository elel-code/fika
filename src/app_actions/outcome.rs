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

    pub(crate) fn merge(self, supplemental: Self) -> Self {
        match (self, supplemental) {
            (Self::None, outcome) | (outcome, Self::None) => outcome,
            (Self::Present(reason), _) | (_, Self::Present(reason)) => Self::Present(reason),
            (
                Self::Queue {
                    reason,
                    redraw_frames,
                },
                Self::Queue {
                    redraw_frames: supplemental_frames,
                    ..
                },
            ) => Self::Queue {
                reason,
                redraw_frames: redraw_frames.max(supplemental_frames),
            },
            (queue @ Self::Queue { .. }, _) | (_, queue @ Self::Queue { .. }) => queue,
            (Self::Redraw, Self::Redraw) => Self::Redraw,
        }
    }

    pub(crate) fn with_redraw_if(self, changed: bool) -> Self {
        self.merge(Self::redraw_if(changed))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_merge_keeps_strongest_presentation_level() {
        assert_eq!(
            ShellActionOutcome::Redraw.merge(ShellActionOutcome::None),
            ShellActionOutcome::Redraw
        );
        assert_eq!(
            ShellActionOutcome::Redraw.merge(ShellActionOutcome::Queue {
                reason: "scroll",
                redraw_frames: 2,
            }),
            ShellActionOutcome::Queue {
                reason: "scroll",
                redraw_frames: 2,
            }
        );
        assert_eq!(
            ShellActionOutcome::Queue {
                reason: "scroll",
                redraw_frames: 2,
            }
            .merge(ShellActionOutcome::Present("view-mode")),
            ShellActionOutcome::Present("view-mode")
        );
    }

    #[test]
    fn outcome_merge_coalesces_queue_frames_without_losing_primary_reason() {
        assert_eq!(
            ShellActionOutcome::Queue {
                reason: "primary",
                redraw_frames: 2,
            }
            .merge(ShellActionOutcome::Queue {
                reason: "supplemental",
                redraw_frames: 5,
            }),
            ShellActionOutcome::Queue {
                reason: "primary",
                redraw_frames: 5,
            }
        );
    }

    #[test]
    fn outcome_with_redraw_if_only_upgrades_empty_outcomes() {
        assert_eq!(
            ShellActionOutcome::None.with_redraw_if(true),
            ShellActionOutcome::Redraw
        );
        assert_eq!(
            ShellActionOutcome::Present("present").with_redraw_if(true),
            ShellActionOutcome::Present("present")
        );
        assert_eq!(
            ShellActionOutcome::Queue {
                reason: "queue",
                redraw_frames: 3,
            }
            .with_redraw_if(false),
            ShellActionOutcome::Queue {
                reason: "queue",
                redraw_frames: 3,
            }
        );
    }
}
