impl ShellScene {
    fn is_overflow_menu_open(&self) -> bool {
        self.overflow_menu.is_some()
    }

    fn toggle_overflow_menu(&mut self, size: PhysicalSize<u32>) -> bool {
        if self.overflow_menu.take().is_some() {
            fika_log!("[fika-wgpu] overflow-menu open=0");
            return true;
        }
        self.context_menu = None;
        self.context_menu_safe_triangle.reset();
        self.drop_menu = None;
        self.overflow_menu = Some(ShellOverflowMenu::new(self.overflow_button_rect(size)));
        fika_log!(
            "[fika-wgpu] overflow-menu open=1 actions={}",
            overflow_menu_items(
                self.show_hidden,
                self.places_visible,
                self.dark_mode,
                self.background_blur,
                self.window_opacity,
            )
            .len()
        );
        true
    }

    fn close_overflow_menu(&mut self) -> bool {
        if self.overflow_menu.take().is_none() {
            return false;
        }
        fika_log!("[fika-wgpu] overflow-menu open=0");
        true
    }

    fn sync_overflow_menu_anchor(&mut self, size: PhysicalSize<u32>) -> bool {
        let anchor = self.overflow_button_rect(size);
        let Some(menu) = self.overflow_menu.as_mut() else {
            return false;
        };
        let changed = menu.anchor != anchor;
        menu.anchor = anchor;
        changed
    }

    fn update_overflow_menu_hover(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> bool {
        let row = self.overflow_menu_row_at_screen_point(point, size);
        let Some(menu) = self.overflow_menu.as_mut() else {
            return false;
        };
        let changed = menu.hovered_row != row;
        menu.hovered_row = row;
        changed
    }

    fn overflow_menu_row_at_screen_point(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<usize> {
        overflow_menu_row_at_screen_point(
            self.overflow_menu.as_ref()?,
            point,
            size,
            self.ui_scale(),
        )
    }

    fn activate_or_close_overflow_menu(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Option<ShellOverflowMenuAction> {
        let mut action = self
            .overflow_menu_row_at_screen_point(point, size)
            .and_then(|row| {
                overflow_menu_items(
                    self.show_hidden,
                    self.places_visible,
                    self.dark_mode,
                    self.background_blur,
                    self.window_opacity,
                )
                .get(row)
                .map(|item| item.action)
            });
        if matches!(action, Some(ShellOverflowMenuAction::SetWindowOpacity(_))) {
            action = opacity_percent_at_screen_point(
                self.overflow_menu.as_ref()?,
                point,
                size,
                self.ui_scale(),
            )
            .map(ShellOverflowMenuAction::SetWindowOpacity)
            .or(action);
        }
        let keep_open = matches!(action, Some(ShellOverflowMenuAction::SetWindowOpacity(_)));
        if keep_open {
            if let Some(action) = action {
                self.overflow_menu_actions += 1;
                fika_log!(
                    "[fika-wgpu] overflow-menu action={} actions={}",
                    action.as_str(),
                    self.overflow_menu_actions
                );
            }
            return action;
        }
        let menu_was_open = self.overflow_menu.take().is_some();
        if let Some(action) = action {
            self.overflow_menu_actions += 1;
            fika_log!(
                "[fika-wgpu] overflow-menu action={} actions={}",
                action.as_str(),
                self.overflow_menu_actions
            );
            Some(action)
        } else {
            if menu_was_open {
                fika_log!("[fika-wgpu] overflow-menu open=0");
            }
            None
        }
    }

    fn toggle_background_blur(&mut self) -> bool {
        self.background_blur = !self.background_blur;
        self.appearance_changes += 1;
        true
    }

    fn set_window_opacity_percent(&mut self, percent: u8) -> bool {
        let percent = window_opacity_percent(percent as f32 / 100.0);
        let opacity = percent as f32 / 100.0;
        if (self.window_opacity - opacity).abs() <= f32::EPSILON {
            return false;
        }
        self.window_opacity = opacity;
        self.appearance_changes += 1;
        true
    }
}
