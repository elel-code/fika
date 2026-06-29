use winit::event::{ElementState, KeyEvent};
use winit::event_loop::ActiveEventLoop;

use crate::shell::shortcuts::{
    LocationCommand, dark_mode_toggle_requested_for_key_event, escape_requested_for_key_event,
    file_keyboard_command_for_key_event, filter_command_for_key_event,
    hidden_toggle_requested_for_key_event, is_activation_key, location_command_for_key_event,
    navigation_action_for_key, path_navigation_action_for_key, reload_requested_for_key_event,
    selection_command_for_key_event, view_mode_for_key_event, zoom_action_for_key_event,
};
use crate::{FikaWgpuApp, VIEW_SWITCH_REDRAW_FRAMES, ZOOM_REDRAW_FRAMES, window_title};

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
        if self.scene.is_trash_conflict_dialog_open() {
            if escape_requested_for_key_event(event) {
                if self.scene.close_trash_conflict_dialog()
                    && let Some(window) = self.window.as_ref()
                {
                    window.request_redraw();
                }
            }
            return;
        }
        if self.scene.is_task_detail_dialog_open() {
            if escape_requested_for_key_event(event) {
                if self.scene.close_task_detail_dialog()
                    && let Some(window) = self.window.as_ref()
                {
                    window.request_redraw();
                }
            }
            return;
        }
        if self.scene.is_properties_overlay_open() && escape_requested_for_key_event(event) {
            if self.scene.close_properties_overlay()
                && let Some(window) = self.window.as_ref()
            {
                window.request_redraw();
            }
            return;
        }
        if self.scene.is_drop_menu_open() && escape_requested_for_key_event(event) {
            if self.scene.close_drop_menu()
                && let Some(window) = self.window.as_ref()
            {
                window.request_redraw();
            }
            return;
        }
        if self.scene.is_context_menu_open() && escape_requested_for_key_event(event) {
            if self.scene.close_context_menu()
                && let Some(window) = self.window.as_ref()
            {
                window.request_redraw();
            }
            return;
        }
        if dark_mode_toggle_requested_for_key_event(
            event,
            shortcut,
            self.modifiers.state().shift_key(),
        ) {
            if self.toggle_user_dark_mode() {
                self.present_scene_change(event_loop, "toggle-dark-mode");
            }
            return;
        }
        if let Some(command) =
            location_command_for_key_event(event, shortcut, self.scene.is_location_editing())
        {
            if command == LocationCommand::Commit {
                self.commit_location_draft(event_loop);
            } else if self.scene.apply_location_command(command, size)
                && let Some(window) = self.window.as_ref()
            {
                window.request_redraw();
            }
            return;
        }
        if let Some(command) =
            filter_command_for_key_event(event, shortcut, self.scene.filter_active)
        {
            if self.scene.apply_filter_command(command, size)
                && let Some(window) = self.window.as_ref()
            {
                window.request_redraw();
            }
            return;
        }
        if let Some(command) = file_keyboard_command_for_key_event(event, shortcut) {
            self.perform_file_keyboard_command(event_loop, command);
            return;
        }
        if let Some(view_mode) = view_mode_for_key_event(event, shortcut) {
            if self.set_user_view_mode(view_mode, size) {
                self.pending_redraw_frames = VIEW_SWITCH_REDRAW_FRAMES;
                if let Some(window) = self.window.as_ref() {
                    window.set_title(&window_title(&self.scene));
                    window.request_redraw();
                }
                self.prewarm_current_scene_caches("switch-immediate");
                self.render_now(event_loop, "switch-immediate", true);
            }
            return;
        }
        if shortcut && let Some(zoom_action) = zoom_action_for_key_event(event) {
            if self.scene.zoom(zoom_action, size) {
                self.queue_scene_change("zoom", ZOOM_REDRAW_FRAMES);
            }
            return;
        }
        if let Some(command) = selection_command_for_key_event(event, shortcut) {
            if self.scene.apply_selection_command(command)
                && let Some(window) = self.window.as_ref()
            {
                window.request_redraw();
            }
            return;
        }
        if reload_requested_for_key_event(event, shortcut) {
            self.reload_scene_path(event_loop);
            return;
        }
        if hidden_toggle_requested_for_key_event(event, shortcut) {
            if self.toggle_user_hidden_visibility(size) {
                self.present_scene_change(event_loop, "toggle-hidden");
            }
            return;
        }
        if is_activation_key(&event.logical_key) {
            if let Some((pane, path)) = self.scene.selected_directory_path() {
                self.load_path_into_pane(event_loop, pane, path, "activate-directory");
            } else if let Some(request) = self.scene.selected_file_open_request() {
                self.launch_open_file_request(&request);
            }
            return;
        }
        if let Some(action) =
            path_navigation_action_for_key(&event.logical_key, self.modifiers.state().alt_key())
        {
            self.perform_path_navigation(event_loop, action);
            return;
        }
        let Some(action) = navigation_action_for_key(&event.logical_key) else {
            return;
        };
        let extend = self.modifiers.state().shift_key();
        if self.scene.navigate(action, extend, size)
            && let Some(window) = self.window.as_ref()
        {
            window.request_redraw();
        }
    }
}
