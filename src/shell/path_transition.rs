use std::time::{Duration, Instant};

use fika_core::ViewRect;

use crate::ShellScene;
use crate::shell::metrics::{
    PATH_TRANSITION_ANIMATION_DURATION, PATH_TRANSITION_ANIMATION_FRAME,
    PATH_TRANSITION_APPEAR_DELAY, PATH_TRANSITION_ENTER_OPACITY, PATH_TRANSITION_ENTER_SCALE,
};
use crate::shell::pane::{
    ShellPaneGeometry, ShellPaneId, ShellPaneScrollMetrics, ShellPaneState, ShellPaneVisibleItem,
};

#[derive(Default)]
pub(crate) struct ShellPathTransitionRuntime {
    transitions: Vec<ShellPathTransition>,
    generation: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct ShellPathTransitionSnapshot {
    pub(crate) state: ShellPaneState,
    pub(crate) geometry: ShellPaneGeometry,
    pub(crate) visible_items: Vec<ShellPaneVisibleItem>,
    pub(crate) scroll_metrics: ShellPaneScrollMetrics,
}

struct ShellPathTransition {
    pane: ShellPaneId,
    started: Instant,
    exit_snapshot: Option<ShellPathTransitionSnapshot>,
}

impl ShellPathTransitionSnapshot {
    pub(crate) fn new(
        state: ShellPaneState,
        geometry: ShellPaneGeometry,
        visible_items: Vec<ShellPaneVisibleItem>,
        scroll_metrics: ShellPaneScrollMetrics,
    ) -> Self {
        Self {
            state,
            geometry,
            visible_items,
            scroll_metrics,
        }
    }
}

impl ShellPathTransitionRuntime {
    pub(crate) fn start(
        &mut self,
        pane: ShellPaneId,
        exit_snapshot: Option<ShellPathTransitionSnapshot>,
    ) {
        self.transitions
            .retain(|transition| transition.pane != pane);
        self.transitions.push(ShellPathTransition {
            pane,
            started: Instant::now(),
            exit_snapshot,
        });
        self.bump_generation();
    }

    fn enter_process_for_pane_at(&self, pane: ShellPaneId, now: Instant) -> f32 {
        self.transitions
            .iter()
            .find(|transition| transition.pane == pane)
            .map(|transition| transition.enter_process(now))
            .unwrap_or(1.0)
    }

    fn exit_process_for_pane_at(&self, pane: ShellPaneId, now: Instant) -> Option<f32> {
        self.transitions
            .iter()
            .find(|transition| transition.pane == pane && transition.exit_snapshot.is_some())
            .and_then(|transition| {
                let process = transition.exit_process(now);
                (process > 0.0).then_some(process)
            })
    }

    fn exit_snapshot_for_pane(&self, pane: ShellPaneId) -> Option<&ShellPathTransitionSnapshot> {
        self.transitions
            .iter()
            .find(|transition| transition.pane == pane && transition.exit_snapshot.is_some())
            .and_then(|transition| transition.exit_snapshot.as_ref())
    }

    pub(crate) fn active_at(&self, now: Instant) -> bool {
        self.transitions
            .iter()
            .any(|transition| transition.active(now))
    }

    pub(crate) fn next_frame_deadline(&self, now: Instant) -> Option<Instant> {
        self.active_at(now)
            .then_some(now + PATH_TRANSITION_ANIMATION_FRAME)
    }

    pub(crate) fn prune_finished(&mut self, now: Instant) -> bool {
        let old_len = self.transitions.len();
        self.transitions.retain(|transition| transition.active(now));
        if self.transitions.len() == old_len {
            return false;
        }
        self.bump_generation();
        true
    }

    pub(crate) fn dirty_value(&self) -> u64 {
        if self.transitions.is_empty() {
            return self.generation << 32;
        }
        let now = Instant::now();
        let frame_ms = PATH_TRANSITION_ANIMATION_FRAME.as_millis().max(1) as u64;
        let frame = self
            .transitions
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

    fn bump_generation(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }
}

impl ShellPathTransition {
    fn enter_process(&self, now: Instant) -> f32 {
        let elapsed = now.duration_since(self.started);
        if elapsed < PATH_TRANSITION_APPEAR_DELAY {
            return 0.0;
        }
        Self::animated_process(elapsed - PATH_TRANSITION_APPEAR_DELAY)
    }

    fn exit_process(&self, now: Instant) -> f32 {
        1.0 - Self::animated_process(now.duration_since(self.started))
    }

    fn animated_process(elapsed: Duration) -> f32 {
        if elapsed >= PATH_TRANSITION_ANIMATION_DURATION {
            return 1.0;
        }
        let duration = PATH_TRANSITION_ANIMATION_DURATION
            .as_secs_f32()
            .max(f32::EPSILON);
        let t = (elapsed.as_secs_f32() / duration).clamp(0.0, 1.0);
        qt_out_expo(t)
    }

    fn active(&self, now: Instant) -> bool {
        now.duration_since(self.started)
            < PATH_TRANSITION_APPEAR_DELAY + PATH_TRANSITION_ANIMATION_DURATION
    }
}

pub(crate) fn start_path_transition(
    scene: &mut ShellScene,
    pane: ShellPaneId,
    exit_snapshot: Option<ShellPathTransitionSnapshot>,
) {
    scene.path_transition.start(pane, exit_snapshot);
}

pub(crate) fn path_transition_active(scene: &ShellScene) -> bool {
    scene.path_transition.active_at(Instant::now())
}

pub(crate) fn next_path_transition_frame_deadline(scene: &ShellScene) -> Option<Instant> {
    scene.path_transition.next_frame_deadline(Instant::now())
}

pub(crate) fn prune_finished_path_transitions(scene: &mut ShellScene) -> bool {
    scene.path_transition.prune_finished(Instant::now())
}

pub(crate) fn path_transition_dirty_value(scene: &ShellScene) -> u64 {
    scene.path_transition.dirty_value()
}

pub(crate) fn enter_process_for_pane(scene: &ShellScene, pane: ShellPaneId) -> f32 {
    scene
        .path_transition
        .enter_process_for_pane_at(pane, Instant::now())
}

pub(crate) fn exit_process_for_pane(scene: &ShellScene, pane: ShellPaneId) -> Option<f32> {
    scene
        .path_transition
        .exit_process_for_pane_at(pane, Instant::now())
}

pub(crate) fn exit_snapshot_for_pane(
    scene: &ShellScene,
    pane: ShellPaneId,
) -> Option<&ShellPathTransitionSnapshot> {
    scene.path_transition.exit_snapshot_for_pane(pane)
}

pub(crate) fn opacity_for_process(process: f32) -> f32 {
    PATH_TRANSITION_ENTER_OPACITY + (1.0 - PATH_TRANSITION_ENTER_OPACITY) * process.clamp(0.0, 1.0)
}

pub(crate) fn transform_rect_for_process(
    rect: ViewRect,
    content: ViewRect,
    process: f32,
) -> ViewRect {
    if process >= 1.0 {
        return rect;
    }
    let scale = PATH_TRANSITION_ENTER_SCALE + (1.0 - PATH_TRANSITION_ENTER_SCALE) * process;
    scale_rect_around(rect, content_center(content), scale)
}

#[cfg(test)]
pub(crate) fn has_active_path_transition(scene: &ShellScene) -> bool {
    !scene.path_transition.transitions.is_empty()
}

#[cfg(test)]
pub(crate) fn path_transition_exit_snapshot_entry_count(
    scene: &ShellScene,
    pane: ShellPaneId,
) -> Option<usize> {
    exit_snapshot_for_pane(scene, pane).map(|snapshot| snapshot.state.entries.len())
}

fn scale_rect_around(rect: ViewRect, center: (f32, f32), scale: f32) -> ViewRect {
    let x = center.0 + (rect.x - center.0) * scale;
    let y = center.1 + (rect.y - center.1) * scale;
    ViewRect {
        x,
        y,
        width: rect.width * scale,
        height: rect.height * scale,
    }
}

fn content_center(content: ViewRect) -> (f32, f32) {
    (
        content.x + content.width / 2.0,
        content.y + content.height / 2.0,
    )
}

fn qt_out_expo(t: f32) -> f32 {
    if t >= 1.0 {
        1.0
    } else if t <= 0.0 {
        0.0
    } else {
        1.0 - 2.0_f32.powf(-10.0 * t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_transition_matches_deepin_enter_animation_config() {
        assert_eq!(
            PATH_TRANSITION_ANIMATION_DURATION,
            Duration::from_millis(200)
        );
        assert_eq!(PATH_TRANSITION_APPEAR_DELAY, Duration::from_millis(100));
        assert_close(PATH_TRANSITION_ENTER_SCALE, 0.8);
        assert_close(PATH_TRANSITION_ENTER_OPACITY, 0.0);
        assert_close(qt_out_expo(0.5), 0.96875);
    }

    #[test]
    fn path_transition_uses_deepin_disappear_then_delayed_appear_timing() {
        let started = Instant::now();
        let transition = ShellPathTransition {
            pane: ShellPaneId::SLOT_0,
            started,
            exit_snapshot: None,
        };

        assert_close(transition.exit_process(started), 1.0);
        assert_close(
            transition.exit_process(started + Duration::from_millis(100)),
            0.03125,
        );
        assert_close(
            transition.enter_process(started + Duration::from_millis(99)),
            0.0,
        );
        assert_close(
            transition.enter_process(started + Duration::from_millis(200)),
            0.96875,
        );
        assert_close(
            transition.enter_process(started + Duration::from_millis(300)),
            1.0,
        );
        assert!(!transition.active(started + Duration::from_millis(300)));
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 0.000_01,
            "actual={actual} expected={expected}"
        );
    }
}
