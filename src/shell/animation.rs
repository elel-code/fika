use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use fika_core::ViewRect;

use crate::shell::metrics::{
    ITEM_REFLOW_ANIMATION_DURATION, ITEM_REFLOW_ANIMATION_FRAME, TEXT_CARET_BLINK_INTERVAL,
};
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
    text_caret_blink: ShellTextCaretBlinkRuntime,
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

    pub(crate) fn reset_text_caret_blink(&mut self) {
        self.text_caret_blink.reset();
    }

    pub(crate) fn text_caret_visible(&self) -> bool {
        self.text_caret_blink.visible()
    }

    pub(crate) fn text_caret_dirty_value(&self, active: bool) -> u64 {
        self.text_caret_blink.dirty_value(active)
    }

    pub(crate) fn next_text_caret_blink_deadline(&self, active: bool) -> Option<Instant> {
        self.text_caret_blink.next_deadline(active)
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

#[derive(Clone, Debug)]
struct ShellTextCaretBlinkRuntime {
    started: Instant,
    generation: u64,
}

impl Default for ShellTextCaretBlinkRuntime {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            generation: 0,
        }
    }
}

impl ShellTextCaretBlinkRuntime {
    fn reset(&mut self) {
        self.started = Instant::now();
        self.generation = self.generation.wrapping_add(1);
    }

    fn visible(&self) -> bool {
        self.visible_at(Instant::now())
    }

    fn visible_at(&self, now: Instant) -> bool {
        self.phase_at(now) % 2 == 0
    }

    fn dirty_value(&self, active: bool) -> u64 {
        if !active {
            return 0;
        }
        self.dirty_value_at(Instant::now())
    }

    fn dirty_value_at(&self, now: Instant) -> u64 {
        (self.generation << 32) ^ self.phase_at(now)
    }

    fn next_deadline(&self, active: bool) -> Option<Instant> {
        active.then(|| self.next_deadline_at(Instant::now()))
    }

    fn next_deadline_at(&self, now: Instant) -> Instant {
        let interval_ms = TEXT_CARET_BLINK_INTERVAL.as_millis().max(1);
        let elapsed_ms = now.saturating_duration_since(self.started).as_millis();
        let next_elapsed_ms = ((elapsed_ms / interval_ms) + 1) * interval_ms;
        self.started + Duration::from_millis(next_elapsed_ms.min(u64::MAX as u128) as u64)
    }

    fn phase_at(&self, now: Instant) -> u64 {
        let interval_ms = TEXT_CARET_BLINK_INTERVAL.as_millis().max(1);
        let phase = now.saturating_duration_since(self.started).as_millis() / interval_ms;
        phase.min(u64::MAX as u128) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_caret_blink_toggles_on_interval_and_reset_generation() {
        let mut blink = ShellTextCaretBlinkRuntime::default();
        let started = blink.started;
        assert!(blink.visible_at(started));
        assert!(blink.visible_at(started + TEXT_CARET_BLINK_INTERVAL / 2));
        assert!(!blink.visible_at(started + TEXT_CARET_BLINK_INTERVAL));
        assert!(blink.visible_at(started + TEXT_CARET_BLINK_INTERVAL + TEXT_CARET_BLINK_INTERVAL));

        let first_dirty = blink.dirty_value_at(started + TEXT_CARET_BLINK_INTERVAL);
        blink.reset();
        assert!(blink.visible());
        assert_ne!(blink.dirty_value_at(blink.started), first_dirty);
        assert_eq!(blink.dirty_value(false), 0);
    }

    #[test]
    fn text_caret_next_deadline_advances_by_blink_phase() {
        let blink = ShellTextCaretBlinkRuntime::default();
        let started = blink.started;
        assert_eq!(
            blink.next_deadline_at(started),
            started + TEXT_CARET_BLINK_INTERVAL
        );
        assert_eq!(
            blink.next_deadline_at(started + TEXT_CARET_BLINK_INTERVAL),
            started + TEXT_CARET_BLINK_INTERVAL + TEXT_CARET_BLINK_INTERVAL
        );
        assert!(blink.next_deadline(true).is_some());
        assert!(blink.next_deadline(false).is_none());
    }
}
