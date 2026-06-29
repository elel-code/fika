use std::time::Instant;

use winit::cursor::CursorIcon;
use winit::dpi::PhysicalPosition;
use winit::event::{ButtonSource, ElementState, MouseButton};
use winit::event_loop::ActiveEventLoop;

use super::outcome::ShellActionOutcome;
use crate::shell::selection::SelectionClick;
use crate::shell::shortcuts::path_navigation_action_for_mouse_button;
use crate::shell::tasks::TaskDetailDialogClick;
use crate::shell::trash_conflict::TrashConflictDialogClick;
use crate::{
    FikaWgpuApp, ShellItemActivation, ShellPlaceActivation, view_point_from_physical_position,
};

impl FikaWgpuApp {
    pub(crate) fn handle_main_pointer_moved(&mut self, position: PhysicalPosition<f64>) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let point = view_point_from_physical_position(position);
        if self.scene.is_task_detail_dialog_open() {
            self.set_window_cursor(CursorIcon::Default);
            return;
        }
        let changed = self.scene.set_pointer(point, size);
        self.update_window_cursor_for_scene(size);
        self.apply_window_action_outcome(ShellActionOutcome::redraw_if(changed));
    }

    pub(crate) fn handle_main_pointer_left(&mut self) {
        self.set_window_cursor(CursorIcon::Default);
        let changed = self.scene.clear_pointer();
        self.apply_window_action_outcome(ShellActionOutcome::redraw_if(changed));
    }

    pub(crate) fn handle_main_pointer_button(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        state: ElementState,
        position: PhysicalPosition<f64>,
        button: ButtonSource,
    ) {
        let Some(size) = self.renderer.as_ref().map(|renderer| renderer.size) else {
            return;
        };
        let point = view_point_from_physical_position(position);
        let Some(mouse_button) = button.mouse_button() else {
            return;
        };

        if self.scene.is_trash_conflict_dialog_open() {
            self.handle_trash_conflict_pointer_button(event_loop, state, mouse_button, point, size);
            return;
        }
        if self.scene.is_task_detail_dialog_open() {
            self.handle_task_detail_pointer_button(state, mouse_button, point, size);
            return;
        }
        if self.scene.is_properties_overlay_open() {
            self.handle_properties_overlay_pointer_button(state, mouse_button, point, size);
            return;
        }
        if state == ElementState::Pressed
            && let Some(action) = path_navigation_action_for_mouse_button(mouse_button)
        {
            self.perform_path_navigation(event_loop, action);
            return;
        }
        if mouse_button == MouseButton::Right {
            if state == ElementState::Pressed {
                let changed =
                    self.scene
                        .open_context_menu_with_cache(point, size, &self.mime_applications);
                self.apply_window_action_outcome(ShellActionOutcome::redraw_if(changed));
            }
            return;
        }
        if mouse_button != MouseButton::Left {
            return;
        }

        self.handle_main_left_pointer_button(event_loop, state, point, size);
    }

    fn handle_trash_conflict_pointer_button(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        state: ElementState,
        mouse_button: MouseButton,
        point: crate::ViewPoint,
        size: winit::dpi::PhysicalSize<u32>,
    ) {
        if state != ElementState::Pressed || mouse_button != MouseButton::Left {
            return;
        }
        match self
            .scene
            .trash_conflict_dialog_click_at_screen_point(point, size)
        {
            TrashConflictDialogClick::Outside | TrashConflictDialogClick::Cancel => {
                let changed = self.scene.close_trash_conflict_dialog();
                self.apply_window_action_outcome(ShellActionOutcome::redraw_if(changed));
            }
            TrashConflictDialogClick::Replace => {
                self.replace_trash_restore_conflicts(event_loop);
            }
            TrashConflictDialogClick::Inside => {}
        }
    }

    fn handle_task_detail_pointer_button(
        &mut self,
        state: ElementState,
        mouse_button: MouseButton,
        point: crate::ViewPoint,
        size: winit::dpi::PhysicalSize<u32>,
    ) {
        if state != ElementState::Pressed || mouse_button != MouseButton::Left {
            return;
        }
        let changed = match self
            .scene
            .task_detail_dialog_click_at_screen_point(point, size)
        {
            TaskDetailDialogClick::Outside | TaskDetailDialogClick::Cancel => {
                self.scene.close_task_detail_dialog()
            }
            TaskDetailDialogClick::Clear => self.scene.clear_task_statuses(),
            TaskDetailDialogClick::Dismiss(index) => {
                let (changed, task_id) = self.scene.dismiss_task_status(index);
                if let Some(task_id) = task_id {
                    self.cancel_task_if_running(task_id);
                }
                changed
            }
            TaskDetailDialogClick::Inside => false,
        };
        self.apply_window_action_outcome(ShellActionOutcome::redraw_if(changed));
    }

    fn handle_properties_overlay_pointer_button(
        &mut self,
        state: ElementState,
        mouse_button: MouseButton,
        point: crate::ViewPoint,
        size: winit::dpi::PhysicalSize<u32>,
    ) {
        if state == ElementState::Pressed && mouse_button == MouseButton::Left {
            let changed = self.scene.close_properties_overlay_if_outside(point, size);
            self.apply_window_action_outcome(ShellActionOutcome::redraw_if(changed));
        }
    }

    fn handle_main_left_pointer_button(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        state: ElementState,
        point: crate::ViewPoint,
        size: winit::dpi::PhysicalSize<u32>,
    ) {
        let path_bar_hit = state == ElementState::Pressed
            && self.scene.path_bar_contains_screen_point(point, size);
        let location_blur_changed = state == ElementState::Pressed
            && !path_bar_hit
            && self.scene.close_location_draft_if_outside(point, size);

        if state == ElementState::Released && self.scene.is_scrollbar_dragging() {
            let changed = self.scene.end_scrollbar_drag(point, size);
            self.update_window_cursor_for_scene(size);
            self.apply_window_action_outcome(ShellActionOutcome::redraw_if(changed));
            return;
        }
        if state == ElementState::Pressed && self.scene.is_context_menu_open() {
            self.activate_or_close_context_menu(event_loop, point, size);
            return;
        }
        if state == ElementState::Pressed && self.scene.is_drop_menu_open() {
            self.activate_or_close_drop_menu(event_loop, point, size);
            return;
        }
        if state == ElementState::Pressed
            && let Some(changed) = self
                .scene
                .open_task_detail_dialog_at_screen_point(point, size)
        {
            self.apply_window_action_outcome(ShellActionOutcome::redraw_if(
                changed || location_blur_changed,
            ));
            return;
        }
        if state == ElementState::Pressed
            && self.scene.split_view_button_at_screen_point(point, size)
        {
            self.toggle_split_view_from_toolbar(event_loop);
            return;
        }
        if state == ElementState::Pressed
            && let Some(changed) = self.scene.toggle_places_at_screen_point(point, size)
        {
            self.update_window_cursor_for_scene(size);
            self.apply_window_action_outcome(ShellActionOutcome::redraw_if(
                changed || location_blur_changed,
            ));
            return;
        }
        if state == ElementState::Pressed
            && let Some(changed) = self.scene.begin_scrollbar_drag(point, size)
        {
            self.update_window_cursor_for_scene(size);
            self.apply_window_action_outcome(ShellActionOutcome::redraw_if(
                changed || location_blur_changed,
            ));
            return;
        }
        if path_bar_hit {
            let changed = self.scene.activate_path_bar_at_screen_point(point, size);
            self.apply_window_action_outcome(ShellActionOutcome::redraw_if(changed));
            return;
        }
        if state == ElementState::Pressed
            && let Some(action) = self
                .scene
                .path_navigation_action_at_screen_point(point, size)
        {
            self.perform_path_navigation(event_loop, action);
            return;
        }
        if state == ElementState::Pressed
            && let Some(view_mode) = self.scene.view_mode_at_screen_point(point, size)
        {
            let changed = self.set_user_view_mode(view_mode, size);
            self.apply_action_outcome(
                event_loop,
                ShellActionOutcome::present_if(changed, "mode-click"),
            );
            return;
        }
        if state == ElementState::Pressed
            && let Some(changed) = self.scene.begin_place_pointer(point, size)
        {
            self.apply_window_action_outcome(ShellActionOutcome::redraw_if(
                changed || location_blur_changed,
            ));
            return;
        }
        if state == ElementState::Released && self.scene.place_pointer_active() {
            self.end_place_pointer(event_loop, point, size, location_blur_changed);
            return;
        }
        if state == ElementState::Pressed
            && let Some(activation) =
                self.scene
                    .item_activation_for_press(point, size, Instant::now())
        {
            self.perform_item_activation(event_loop, activation);
            return;
        }

        let changed = match state {
            ElementState::Pressed => {
                let selection = SelectionClick {
                    point,
                    extend: self.modifiers.state().shift_key(),
                    toggle: self.modifiers.state().control_key()
                        || self.modifiers.state().meta_key(),
                };
                self.scene.begin_pane_pointer(selection, size)
            }
            ElementState::Released => self.scene.end_pane_pointer(point, size),
        };
        self.update_window_cursor_for_scene(size);
        self.apply_window_action_outcome(ShellActionOutcome::redraw_if(
            changed || location_blur_changed,
        ));
    }

    fn activate_or_close_context_menu(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        point: crate::ViewPoint,
        size: winit::dpi::PhysicalSize<u32>,
    ) {
        let action = self
            .scene
            .activate_or_close_context_menu_command(point, size);
        if let Some(action) = action {
            self.perform_context_menu_action(event_loop, action);
        } else {
            self.apply_window_action_outcome(ShellActionOutcome::Redraw);
        }
    }

    fn activate_or_close_drop_menu(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        point: crate::ViewPoint,
        size: winit::dpi::PhysicalSize<u32>,
    ) {
        let request = self.scene.activate_or_close_drop_menu_request(point, size);
        if let Some(request) = request {
            self.perform_drop_operation_request(event_loop, request);
        } else {
            self.apply_window_action_outcome(ShellActionOutcome::Redraw);
        }
    }

    fn end_place_pointer(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        point: crate::ViewPoint,
        size: winit::dpi::PhysicalSize<u32>,
        location_blur_changed: bool,
    ) {
        let (changed, activation) = self.scene.end_place_pointer(point, size);
        if let Some(activation) = activation {
            match activation {
                ShellPlaceActivation::Open { pane, path } => {
                    self.load_path_into_pane(event_loop, pane, path, "place-open");
                }
                ShellPlaceActivation::DeviceAction(request) => {
                    self.perform_device_action_request(event_loop, request);
                }
            }
            return;
        }
        self.apply_window_action_outcome(ShellActionOutcome::redraw_if(
            changed || location_blur_changed,
        ));
    }

    fn perform_item_activation(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        activation: ShellItemActivation,
    ) {
        match activation {
            ShellItemActivation::Directory { pane, path } => {
                self.load_path_into_pane(event_loop, pane, path, "double-click-directory");
            }
            ShellItemActivation::File(request) => self.launch_open_file_request(&request),
        }
    }
}
