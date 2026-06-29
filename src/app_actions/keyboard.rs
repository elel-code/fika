use winit::dpi::PhysicalSize;
use winit::event::{ElementState, KeyEvent};
use winit::event_loop::ActiveEventLoop;

use super::outcome::ShellActionOutcome;
use crate::shell::shortcuts::{
    LocationCommand, dark_mode_toggle_requested_for_key_event, escape_requested_for_key_event,
    file_keyboard_command_for_key_event, filter_command_for_key_event,
    hidden_toggle_requested_for_key_event, is_activation_key, location_command_for_key_event,
    navigation_action_for_key, path_navigation_action_for_key, reload_requested_for_key_event,
    selection_command_for_key_event, view_mode_for_key_event, zoom_action_for_key_event,
};
use crate::{FikaWgpuApp, ZOOM_REDRAW_FRAMES};

impl FikaWgpuApp {
    pub(crate) fn handle_main_keyboard_input(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        event: &KeyEvent,
    ) {
        if event.state != ElementState::Pressed {
            return;
        }
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let shortcut = self.modifiers.state().control_key() || self.modifiers.state().meta_key();
        let outcome = self.dispatch_main_keyboard_input(event_loop, event, size, shortcut);
        self.apply_action_outcome(event_loop, outcome);
    }

    fn dispatch_main_keyboard_input(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        event: &KeyEvent,
        size: PhysicalSize<u32>,
        shortcut: bool,
    ) -> ShellActionOutcome {
        if self.scene.is_trash_conflict_dialog_open() {
            if escape_requested_for_key_event(event) {
                let changed = self.scene.close_trash_conflict_dialog();
                return ShellActionOutcome::redraw_if(changed);
            }
            return ShellActionOutcome::None;
        }
        if self.scene.is_task_detail_dialog_open() {
            if escape_requested_for_key_event(event) {
                let changed = self.scene.close_task_detail_dialog();
                return ShellActionOutcome::redraw_if(changed);
            }
            return ShellActionOutcome::None;
        }
        if self.scene.is_properties_overlay_open() && escape_requested_for_key_event(event) {
            let changed = self.scene.close_properties_overlay();
            return ShellActionOutcome::redraw_if(changed);
        }
        if self.scene.is_drop_menu_open() && escape_requested_for_key_event(event) {
            let changed = self.scene.close_drop_menu();
            return ShellActionOutcome::redraw_if(changed);
        }
        if self.scene.is_context_menu_open() && escape_requested_for_key_event(event) {
            let changed = self.scene.close_context_menu();
            return ShellActionOutcome::redraw_if(changed);
        }
        if dark_mode_toggle_requested_for_key_event(
            event,
            shortcut,
            self.modifiers.state().shift_key(),
        ) {
            let changed = self.toggle_user_dark_mode();
            return ShellActionOutcome::present_if(changed, "toggle-dark-mode");
        }
        if let Some(command) =
            location_command_for_key_event(event, shortcut, self.scene.is_location_editing())
        {
            if command == LocationCommand::Commit {
                self.commit_location_draft(event_loop);
                return ShellActionOutcome::None;
            } else {
                let changed = self.scene.apply_location_command(command, size);
                return ShellActionOutcome::redraw_if(changed);
            }
        }
        if let Some(command) =
            filter_command_for_key_event(event, shortcut, self.scene.filter_active)
        {
            let changed = self.scene.apply_filter_command(command, size);
            return ShellActionOutcome::redraw_if(changed);
        }
        if let Some(command) = file_keyboard_command_for_key_event(event, shortcut) {
            self.perform_file_keyboard_command(event_loop, command);
            return ShellActionOutcome::None;
        }
        if let Some(view_mode) = view_mode_for_key_event(event, shortcut) {
            let changed = self.set_user_view_mode(view_mode, size);
            return ShellActionOutcome::present_if(changed, "switch-immediate");
        }
        if shortcut && let Some(zoom_action) = zoom_action_for_key_event(event) {
            if self.scene.zoom(zoom_action, size) {
                return ShellActionOutcome::Queue {
                    reason: "zoom",
                    redraw_frames: ZOOM_REDRAW_FRAMES,
                };
            }
            return ShellActionOutcome::None;
        }
        if let Some(command) = selection_command_for_key_event(event, shortcut) {
            let changed = self.scene.apply_selection_command(command);
            return ShellActionOutcome::redraw_if(changed);
        }
        if reload_requested_for_key_event(event, shortcut) {
            self.reload_scene_path(event_loop);
            return ShellActionOutcome::None;
        }
        if hidden_toggle_requested_for_key_event(event, shortcut) {
            let changed = self.toggle_user_hidden_visibility(size);
            return ShellActionOutcome::present_if(changed, "toggle-hidden");
        }
        if is_activation_key(&event.logical_key) {
            if let Some((pane, path)) = self.scene.selected_directory_path() {
                self.load_path_into_pane(event_loop, pane, path, "activate-directory");
            } else if let Some(request) = self.scene.selected_file_open_request() {
                self.launch_open_file_request(&request);
            }
            return ShellActionOutcome::None;
        }
        if let Some(action) =
            path_navigation_action_for_key(&event.logical_key, self.modifiers.state().alt_key())
        {
            self.perform_path_navigation(event_loop, action);
            return ShellActionOutcome::None;
        }
        let Some(action) = navigation_action_for_key(&event.logical_key) else {
            return ShellActionOutcome::None;
        };
        let extend = self.modifiers.state().shift_key();
        let changed = self.scene.navigate(action, extend, size);
        ShellActionOutcome::redraw_if(changed)
    }
}
