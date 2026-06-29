use std::path::PathBuf;

use winit::dpi::PhysicalPosition;

use super::outcome::ShellActionOutcome;
use crate::shell::drop_menu::ShellDropTarget;
use crate::shell::tasks::ShellTaskStatus;
use crate::{FikaWgpuApp, view_point_from_physical_position};

impl FikaWgpuApp {
    pub(crate) fn external_drag_entered(
        &mut self,
        paths: Vec<PathBuf>,
        position: PhysicalPosition<f64>,
    ) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let point = view_point_from_physical_position(position);
        let changed = self.scene.begin_external_drag(paths, point, size);
        fika_log!(
            "[fika-wgpu] external-dnd enter sources={} target={}",
            self.scene
                .external_drag
                .as_ref()
                .map(|drag| drag.sources.len())
                .unwrap_or(0),
            self.scene
                .dnd_hover_target
                .as_ref()
                .map(ShellDropTarget::kind)
                .unwrap_or("none")
        );
        self.apply_window_action_outcome(ShellActionOutcome::redraw_if(changed));
    }

    pub(crate) fn external_drag_moved(&mut self, position: PhysicalPosition<f64>) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let point = view_point_from_physical_position(position);
        let changed = self.scene.update_external_drag(point, size);
        self.apply_window_action_outcome(ShellActionOutcome::redraw_if(changed));
    }

    pub(crate) fn external_drag_dropped(
        &mut self,
        paths: Vec<PathBuf>,
        position: PhysicalPosition<f64>,
    ) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let point = view_point_from_physical_position(position);
        let sources = if paths.is_empty() {
            self.scene.external_drag_sources().unwrap_or_default()
        } else {
            paths
        };
        match self.scene.finish_external_drag(sources, point, size) {
            Ok(changed) => {
                fika_log!(
                    "[fika-wgpu] external-dnd drop menu={} target={}",
                    self.scene.drop_menu.is_some() as u8,
                    self.scene
                        .drop_menu
                        .as_ref()
                        .map(|menu| menu.target.kind())
                        .unwrap_or("none")
                );
                self.apply_window_action_outcome(ShellActionOutcome::redraw_if(changed));
            }
            Err(error) => {
                fika_log!("[fika-wgpu] external-dnd-error {error}");
                self.scene
                    .record_task_status(ShellTaskStatus::failed("Drop failed", error, false));
                self.apply_window_action_outcome(ShellActionOutcome::Redraw);
            }
        }
    }

    pub(crate) fn external_drag_left(&mut self) {
        let changed = self.scene.clear_external_drag();
        self.apply_window_action_outcome(ShellActionOutcome::redraw_if(changed));
    }
}
