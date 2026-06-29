use std::time::Instant;

use winit::cursor::CursorIcon;
use winit::dpi::PhysicalPosition;
use winit::event::{ButtonSource, ElementState, MouseButton};
use winit::event_loop::ActiveEventLoop;

use super::outcome::ShellActionOutcome;
use super::pointer_route::{
    MainLeftPointerButtonIntent, MainLeftPointerButtonRoute, MainLeftPointerButtonRouteSnapshot,
    MainPointerButtonIntent, MainPointerButtonSnapshot, main_left_pointer_button_route,
    main_pointer_button_intent,
};
use crate::shell::selection::SelectionClick;
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

        let intent =
            main_pointer_button_intent(state, mouse_button, self.main_pointer_button_snapshot());
        let outcome = self.dispatch_main_pointer_button_intent(
            event_loop,
            intent,
            state,
            mouse_button,
            point,
            size,
        );
        self.apply_action_outcome(event_loop, outcome);
    }

    fn main_pointer_button_snapshot(&self) -> MainPointerButtonSnapshot {
        MainPointerButtonSnapshot {
            trash_conflict_dialog_open: self.scene.is_trash_conflict_dialog_open(),
            task_detail_dialog_open: self.scene.is_task_detail_dialog_open(),
            properties_overlay_open: self.scene.is_properties_overlay_open(),
        }
    }

    fn dispatch_main_pointer_button_intent(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        intent: MainPointerButtonIntent,
        state: ElementState,
        mouse_button: MouseButton,
        point: crate::ViewPoint,
        size: winit::dpi::PhysicalSize<u32>,
    ) -> ShellActionOutcome {
        match intent {
            MainPointerButtonIntent::TrashConflict => self.handle_trash_conflict_pointer_button(
                event_loop,
                state,
                mouse_button,
                point,
                size,
            ),
            MainPointerButtonIntent::TaskDetail => {
                self.handle_task_detail_pointer_button(state, mouse_button, point, size)
            }
            MainPointerButtonIntent::PropertiesOverlay => {
                self.handle_properties_overlay_pointer_button(state, mouse_button, point, size)
            }
            MainPointerButtonIntent::MouseNavigation(action) => {
                self.perform_path_navigation(event_loop, action);
                ShellActionOutcome::None
            }
            MainPointerButtonIntent::ContextMenu => {
                let changed =
                    self.scene
                        .open_context_menu_with_cache(point, size, &self.mime_applications);
                ShellActionOutcome::redraw_if(changed)
            }
            MainPointerButtonIntent::Left => {
                let route = main_left_pointer_button_route(
                    state,
                    self.main_left_pointer_button_snapshot(point, size),
                );
                self.apply_main_left_pointer_button_route(event_loop, state, point, size, route)
            }
            MainPointerButtonIntent::Ignore => ShellActionOutcome::None,
        }
    }

    fn handle_trash_conflict_pointer_button(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        state: ElementState,
        mouse_button: MouseButton,
        point: crate::ViewPoint,
        size: winit::dpi::PhysicalSize<u32>,
    ) -> ShellActionOutcome {
        if state != ElementState::Pressed || mouse_button != MouseButton::Left {
            return ShellActionOutcome::None;
        }
        match self
            .scene
            .trash_conflict_dialog_click_at_screen_point(point, size)
        {
            TrashConflictDialogClick::Outside | TrashConflictDialogClick::Cancel => {
                let changed = self.scene.close_trash_conflict_dialog();
                ShellActionOutcome::redraw_if(changed)
            }
            TrashConflictDialogClick::Replace => {
                self.replace_trash_restore_conflicts(event_loop);
                ShellActionOutcome::None
            }
            TrashConflictDialogClick::Inside => ShellActionOutcome::None,
        }
    }

    fn handle_task_detail_pointer_button(
        &mut self,
        state: ElementState,
        mouse_button: MouseButton,
        point: crate::ViewPoint,
        size: winit::dpi::PhysicalSize<u32>,
    ) -> ShellActionOutcome {
        if state != ElementState::Pressed || mouse_button != MouseButton::Left {
            return ShellActionOutcome::None;
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
        ShellActionOutcome::redraw_if(changed)
    }

    fn handle_properties_overlay_pointer_button(
        &mut self,
        state: ElementState,
        mouse_button: MouseButton,
        point: crate::ViewPoint,
        size: winit::dpi::PhysicalSize<u32>,
    ) -> ShellActionOutcome {
        if state == ElementState::Pressed && mouse_button == MouseButton::Left {
            let changed = self.scene.close_properties_overlay_if_outside(point, size);
            ShellActionOutcome::redraw_if(changed)
        } else {
            ShellActionOutcome::None
        }
    }

    fn main_left_pointer_button_snapshot(
        &self,
        point: crate::ViewPoint,
        size: winit::dpi::PhysicalSize<u32>,
    ) -> MainLeftPointerButtonRouteSnapshot {
        MainLeftPointerButtonRouteSnapshot {
            scrollbar_dragging: self.scene.is_scrollbar_dragging(),
            context_menu_open: self.scene.is_context_menu_open(),
            drop_menu_open: self.scene.is_drop_menu_open(),
            task_detail_area_hit: self
                .scene
                .task_detail_area_contains_screen_point(point, size),
            split_view_button_hit: self.scene.split_view_button_at_screen_point(point, size),
            places_toggle_hit: self.scene.places_toggle_contains_screen_point(point, size),
            scrollbar_drag_hit: self.scene.scrollbar_drag_hit_at_screen_point(point, size),
            path_bar_hit: self.scene.path_bar_contains_screen_point(point, size),
            toolbar_navigation: self
                .scene
                .path_navigation_action_at_screen_point(point, size),
            view_mode: self.scene.view_mode_at_screen_point(point, size),
            place_pointer_target_hit: self.scene.place_pointer_target_at_screen_point(point, size),
            place_pointer_active: self.scene.place_pointer_active(),
        }
    }

    fn apply_main_left_pointer_button_route(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        state: ElementState,
        point: crate::ViewPoint,
        size: winit::dpi::PhysicalSize<u32>,
        route: MainLeftPointerButtonRoute,
    ) -> ShellActionOutcome {
        let location_blur_changed = route
            .should_blur_location(state)
            .then(|| self.scene.close_location_draft_if_outside(point, size))
            .unwrap_or(false);

        match route.intent {
            MainLeftPointerButtonIntent::EndScrollbarDrag => {
                let changed = self.scene.end_scrollbar_drag(point, size);
                self.update_window_cursor_for_scene(size);
                ShellActionOutcome::redraw_if(changed)
            }
            MainLeftPointerButtonIntent::ContextMenu => {
                self.activate_or_close_context_menu(event_loop, point, size)
            }
            MainLeftPointerButtonIntent::DropMenu => {
                self.activate_or_close_drop_menu(event_loop, point, size)
            }
            MainLeftPointerButtonIntent::OpenTaskDetail => {
                let changed = self
                    .scene
                    .open_task_detail_dialog_at_screen_point(point, size)
                    .unwrap_or(false);
                ShellActionOutcome::redraw_if(changed).with_redraw_if(location_blur_changed)
            }
            MainLeftPointerButtonIntent::ToggleSplitView => {
                self.toggle_split_view_from_toolbar(event_loop);
                ShellActionOutcome::None
            }
            MainLeftPointerButtonIntent::TogglePlaces => {
                let changed = self
                    .scene
                    .toggle_places_at_screen_point(point, size)
                    .unwrap_or(false);
                self.update_window_cursor_for_scene(size);
                ShellActionOutcome::redraw_if(changed).with_redraw_if(location_blur_changed)
            }
            MainLeftPointerButtonIntent::BeginScrollbarDrag => {
                let changed = self
                    .scene
                    .begin_scrollbar_drag(point, size)
                    .unwrap_or(false);
                self.update_window_cursor_for_scene(size);
                ShellActionOutcome::redraw_if(changed).with_redraw_if(location_blur_changed)
            }
            MainLeftPointerButtonIntent::PathBar => {
                let changed = self.scene.activate_path_bar_at_screen_point(point, size);
                ShellActionOutcome::redraw_if(changed)
            }
            MainLeftPointerButtonIntent::ToolbarNavigation(action) => {
                self.perform_path_navigation(event_loop, action);
                ShellActionOutcome::None
            }
            MainLeftPointerButtonIntent::ViewMode(view_mode) => {
                let changed = self.set_user_view_mode(view_mode, size);
                ShellActionOutcome::present_if(changed, "mode-click")
            }
            MainLeftPointerButtonIntent::BeginPlacePointer => {
                let changed = self.scene.begin_place_pointer(point, size).unwrap_or(false);
                ShellActionOutcome::redraw_if(changed).with_redraw_if(location_blur_changed)
            }
            MainLeftPointerButtonIntent::EndPlacePointer => {
                self.end_place_pointer(event_loop, point, size, location_blur_changed)
            }
            MainLeftPointerButtonIntent::ItemActivationCheck => {
                if let Some(activation) =
                    self.scene
                        .item_activation_for_press(point, size, Instant::now())
                {
                    self.perform_item_activation(event_loop, activation);
                    return ShellActionOutcome::None;
                }
                self.apply_pane_pointer(state, point, size, location_blur_changed)
            }
            MainLeftPointerButtonIntent::PanePointer => {
                self.apply_pane_pointer(state, point, size, location_blur_changed)
            }
        }
    }

    fn apply_pane_pointer(
        &mut self,
        state: ElementState,
        point: crate::ViewPoint,
        size: winit::dpi::PhysicalSize<u32>,
        location_blur_changed: bool,
    ) -> ShellActionOutcome {
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
        ShellActionOutcome::redraw_if(changed).with_redraw_if(location_blur_changed)
    }

    fn activate_or_close_context_menu(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        point: crate::ViewPoint,
        size: winit::dpi::PhysicalSize<u32>,
    ) -> ShellActionOutcome {
        let action = self
            .scene
            .activate_or_close_context_menu_command(point, size);
        if let Some(action) = action {
            self.perform_context_menu_action(event_loop, action);
            ShellActionOutcome::None
        } else {
            ShellActionOutcome::Redraw
        }
    }

    fn activate_or_close_drop_menu(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        point: crate::ViewPoint,
        size: winit::dpi::PhysicalSize<u32>,
    ) -> ShellActionOutcome {
        let request = self.scene.activate_or_close_drop_menu_request(point, size);
        if let Some(request) = request {
            self.perform_drop_operation_request(event_loop, request);
            ShellActionOutcome::None
        } else {
            ShellActionOutcome::Redraw
        }
    }

    fn end_place_pointer(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        point: crate::ViewPoint,
        size: winit::dpi::PhysicalSize<u32>,
        location_blur_changed: bool,
    ) -> ShellActionOutcome {
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
            return ShellActionOutcome::None;
        }
        ShellActionOutcome::redraw_if(changed).with_redraw_if(location_blur_changed)
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
