use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use fika_core::ViewRect;

use crate::shell::metrics::{ITEM_REFLOW_ANIMATION_DURATION, ITEM_REFLOW_ANIMATION_FRAME};
use crate::shell::pane::ShellPaneId;

#[derive(Clone, Debug)]
pub(crate) struct ShellItemReflowTransition {
    pub(crate) pane: ShellPaneId,
    pub(crate) path: PathBuf,
    pub(crate) from: ViewRect,
    pub(crate) to: ViewRect,
    started: Instant,
}

impl ShellItemReflowTransition {
    #[cfg(test)]
    pub(crate) fn moved(&self) -> bool {
        item_reflow_rect_moved(self.from, self.to)
    }

    fn offset(&self, now: Instant) -> Option<(f32, f32)> {
        let elapsed = now.duration_since(self.started);
        if elapsed >= ITEM_REFLOW_ANIMATION_DURATION {
            return None;
        }
        let duration = ITEM_REFLOW_ANIMATION_DURATION
            .as_secs_f32()
            .max(f32::EPSILON);
        let t = (elapsed.as_secs_f32() / duration).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - t).powi(3);
        let remaining = 1.0 - eased;
        Some((
            (self.from.x - self.to.x) * remaining,
            (self.from.y - self.to.y) * remaining,
        ))
    }

    fn active(&self, now: Instant) -> bool {
        now.duration_since(self.started) < ITEM_REFLOW_ANIMATION_DURATION
    }
}

#[derive(Default)]
pub(crate) struct ShellAnimationRuntime {
    item_reflow_transitions: Vec<ShellItemReflowTransition>,
    generation: u64,
}

impl ShellAnimationRuntime {
    pub(crate) fn start_item_reflow(
        &mut self,
        pane: ShellPaneId,
        previous_rects: HashMap<PathBuf, ViewRect>,
        next_rects: HashMap<PathBuf, ViewRect>,
    ) -> bool {
        if previous_rects.is_empty() || next_rects.is_empty() {
            return false;
        }
        let started = Instant::now();
        let mut transitions = next_rects
            .into_iter()
            .filter_map(|(path, to)| {
                let from = previous_rects.get(&path).copied()?;
                item_reflow_rect_moved(from, to).then_some(ShellItemReflowTransition {
                    pane,
                    path,
                    from,
                    to,
                    started,
                })
            })
            .collect::<Vec<_>>();
        if transitions.is_empty() {
            return false;
        }
        self.item_reflow_transitions
            .retain(|transition| transition.pane != pane);
        self.item_reflow_transitions.append(&mut transitions);
        self.bump_generation();
        true
    }

    pub(crate) fn item_reflow_offset_for_path(
        &self,
        pane: ShellPaneId,
        path: &Path,
    ) -> Option<(f32, f32)> {
        let now = Instant::now();
        self.item_reflow_transitions
            .iter()
            .find(|transition| transition.pane == pane && transition.path == path)
            .and_then(|transition| transition.offset(now))
    }

    pub(crate) fn active(&self) -> bool {
        let now = Instant::now();
        self.item_reflow_transitions
            .iter()
            .any(|transition| transition.active(now))
    }

    pub(crate) fn next_frame_deadline(&self) -> Option<Instant> {
        self.active()
            .then_some(Instant::now() + ITEM_REFLOW_ANIMATION_FRAME)
    }

    pub(crate) fn prune_finished(&mut self) -> bool {
        if self.item_reflow_transitions.is_empty() {
            return false;
        }
        let now = Instant::now();
        let old_len = self.item_reflow_transitions.len();
        self.item_reflow_transitions
            .retain(|transition| transition.active(now));
        if self.item_reflow_transitions.len() == old_len {
            return false;
        }
        self.bump_generation();
        true
    }

    pub(crate) fn clear(&mut self) {
        if self.item_reflow_transitions.is_empty() {
            return;
        }
        self.item_reflow_transitions.clear();
        self.bump_generation();
    }

    pub(crate) fn dirty_value(&self) -> u64 {
        if self.item_reflow_transitions.is_empty() {
            return self.generation << 32;
        }
        let now = Instant::now();
        let frame_ms = ITEM_REFLOW_ANIMATION_FRAME.as_millis().max(1) as u64;
        let frame = self
            .item_reflow_transitions
            .iter()
            .filter_map(|transition| {
                transition
                    .active(now)
                    .then(|| now.duration_since(transition.started).as_millis() as u64 / frame_ms)
            })
            .min()
            .unwrap_or(0);
        (self.generation << 32) ^ frame
    }

    #[cfg(test)]
    pub(crate) fn item_reflow_transitions(&self) -> &[ShellItemReflowTransition] {
        &self.item_reflow_transitions
    }

    fn bump_generation(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }
}

pub(crate) fn item_reflow_rect_moved(from: ViewRect, to: ViewRect) -> bool {
    (from.x - to.x).abs() >= 0.5 || (from.y - to.y).abs() >= 0.5
}
