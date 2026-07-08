use std::time::Instant;

use cosmic_text::Color as TextColor;
use fika_core::ViewRect;
use winit::dpi::PhysicalSize;

use crate::ShellScene;
use crate::shell::metrics::{
    PATH_TRANSITION_ANIMATION_DURATION, PATH_TRANSITION_ANIMATION_FRAME,
    PATH_TRANSITION_ENTER_SCALE,
};
use crate::shell::pane::{ShellPaneId, ShellPaneProjection};
use crate::shell::render::quad::{QuadVertex, push_clipped_rect};
use crate::shell::theme::ShellTheme;

#[derive(Default)]
pub(crate) struct ShellPathTransitionRuntime {
    transitions: Vec<ShellPathTransition>,
    generation: u64,
}

struct ShellPathTransition {
    pane: ShellPaneId,
    started: Instant,
}

impl ShellPathTransitionRuntime {
    pub(crate) fn start(&mut self, pane: ShellPaneId) {
        self.transitions
            .retain(|transition| transition.pane != pane);
        self.transitions.push(ShellPathTransition {
            pane,
            started: Instant::now(),
        });
        self.bump_generation();
    }

    fn process_for_pane_at(&self, pane: ShellPaneId, now: Instant) -> f32 {
        self.transitions
            .iter()
            .find(|transition| transition.pane == pane)
            .map(|transition| transition.process(now))
            .unwrap_or(1.0)
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
    fn process(&self, now: Instant) -> f32 {
        let elapsed = now.duration_since(self.started);
        if elapsed >= PATH_TRANSITION_ANIMATION_DURATION {
            return 1.0;
        }
        let duration = PATH_TRANSITION_ANIMATION_DURATION
            .as_secs_f32()
            .max(f32::EPSILON);
        let t = (elapsed.as_secs_f32() / duration).clamp(0.0, 1.0);
        1.0 - (1.0 - t).powi(3)
    }

    fn active(&self, now: Instant) -> bool {
        now.duration_since(self.started) < PATH_TRANSITION_ANIMATION_DURATION
    }
}

pub(crate) fn start_path_transition(scene: &mut ShellScene, pane: ShellPaneId) {
    scene.path_transition.start(pane);
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

pub(crate) fn transform_rect_for_pane(
    scene: &ShellScene,
    pane: ShellPaneId,
    rect: ViewRect,
    content: ViewRect,
) -> ViewRect {
    let process = scene
        .path_transition
        .process_for_pane_at(pane, Instant::now());
    if process >= 1.0 {
        return rect;
    }
    let scale = PATH_TRANSITION_ENTER_SCALE + (1.0 - PATH_TRANSITION_ENTER_SCALE) * process;
    scale_rect_around(rect, content_center(content), scale)
}

pub(crate) fn text_color_for_pane(
    scene: &ShellScene,
    pane: ShellPaneId,
    color: TextColor,
) -> TextColor {
    let process = scene
        .path_transition
        .process_for_pane_at(pane, Instant::now());
    if process >= 1.0 {
        return color;
    }
    let [r, g, b, a] = color.as_rgba();
    TextColor::rgba(
        r,
        g,
        b,
        ((a as f32) * process).round().clamp(0.0, 255.0) as u8,
    )
}

pub(crate) fn push_path_transition_overlay(
    scene: &ShellScene,
    vertices: &mut Vec<QuadVertex>,
    projection: &ShellPaneProjection<'_>,
    theme: ShellTheme,
    size: PhysicalSize<u32>,
) {
    let process = scene
        .path_transition
        .process_for_pane_at(projection.geometry.kind, Instant::now());
    if process >= 1.0 {
        return;
    }
    let mut color = theme.view_mode_content(projection.view.view_mode);
    color[3] = 1.0 - process;
    push_clipped_rect(
        vertices,
        projection.geometry.content,
        projection.geometry.content,
        color,
        size,
    );
}

#[cfg(test)]
pub(crate) fn has_active_path_transition(scene: &ShellScene) -> bool {
    !scene.path_transition.transitions.is_empty()
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
