impl ShellScene {

    fn drop_target_at_screen_point_for_drag(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
        drag: &ShellInternalDrag,
    ) -> Option<ShellDropTarget> {
        if let Some(index) = self.place_gap_at_screen_point_for_drag(point, size, drag) {
            let target = ShellDropTarget::PlacesGap { index };
            if self.drag_can_drop_on_target(drag, &target) {
                return Some(target);
            }
        }
        let target = self.drop_target_at_screen_point(point, size)?;
        self.drag_can_drop_on_target(drag, &target)
            .then_some(target)
    }

    fn drag_can_drop_on_target(&self, drag: &ShellInternalDrag, target: &ShellDropTarget) -> bool {
        match target {
            ShellDropTarget::PlacesGap { index } => match &drag.source {
                ShellInternalDragSource::Place {
                    index: source_index,
                } => {
                    self.place_dnd_gap_index_is_valid(*index)
                        && self.place_participates_in_dnd(*source_index)
                        && *index != *source_index
                        && *index != source_index.saturating_add(1)
                }
                ShellInternalDragSource::PaneItem {
                    is_dir: true,
                    source_path,
                    ..
                } => drag.pane_source_directory_path().is_some() && !is_network_path(source_path),
                ShellInternalDragSource::PaneItem { is_dir: false, .. } => false,
            },
            ShellDropTarget::Place { index, .. } => {
                self.place_participates_in_dnd(*index)
                    && matches!(&drag.source, ShellInternalDragSource::PaneItem { .. })
            }
            ShellDropTarget::PaneItem { is_dir, path, .. } if *is_dir => {
                !drag.paths.iter().any(|source| source == path)
            }
            ShellDropTarget::PaneBlank { path, .. } => {
                !drag.paths.iter().any(|source| source == path)
            }
            ShellDropTarget::PaneItem { .. } | ShellDropTarget::PlacesBlank => false,
        }
    }

    fn drop_target_at_screen_point_for_external_drag(
        &self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
        sources: &[PathBuf],
    ) -> Option<ShellDropTarget> {
        let target = self.drop_target_at_screen_point(point, size)?;
        self.external_drag_can_drop_on_target(sources, &target)
            .then_some(target)
    }

    fn external_drag_can_drop_on_target(
        &self,
        sources: &[PathBuf],
        target: &ShellDropTarget,
    ) -> bool {
        if sources.is_empty()
            || sources.iter().any(|path| is_network_path(path))
            || self
                .target_dir_for_drop_target(target)
                .is_some_and(|target_dir| is_network_path(&target_dir))
        {
            return false;
        }
        match target {
            ShellDropTarget::Place { index, path } => {
                self.place_participates_in_dnd(*index)
                    && !sources.iter().any(|source| source == path)
            }
            ShellDropTarget::PaneItem { is_dir, path, .. } if *is_dir => {
                !sources.iter().any(|source| source == path)
            }
            ShellDropTarget::PaneBlank { path, .. } => !sources.iter().any(|source| source == path),
            ShellDropTarget::PaneItem { .. }
            | ShellDropTarget::PlacesGap { .. }
            | ShellDropTarget::PlacesBlank => false,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn update_dnd_hover_target(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        let next = self
            .internal_drag
            .as_ref()
            .and_then(|drag| self.drop_target_at_screen_point_for_drag(point, size, drag));
        let changed = self.dnd_hover_target != next;
        if changed {
            self.dnd_hover_target = next;
            self.dnd_hover_changes += 1;
        }
        changed
    }

    fn begin_external_drag(
        &mut self,
        sources: Vec<PathBuf>,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> bool {
        self.pointer = Some(point);
        self.internal_drag = None;
        self.place_press = None;
        self.rubber_band = None;
        self.context_target = None;
        self.context_menu = None;
        self.drop_menu = None;
        let old_drag = self.external_drag.clone();
        self.external_drag = ShellExternalDrag::new(sources);
        let hover_changed = self.update_external_dnd_hover_target(point, size);
        old_drag != self.external_drag || hover_changed
    }

    fn update_external_drag(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        self.pointer = Some(point);
        self.update_external_dnd_hover_target(point, size)
    }

    fn external_drag_sources(&self) -> Option<Vec<PathBuf>> {
        self.external_drag
            .as_ref()
            .map(|drag| drag.sources.clone())
            .filter(|sources| !sources.is_empty())
    }

    fn clear_external_drag(&mut self) -> bool {
        let changed = self.external_drag.take().is_some() || self.clear_dnd_hover_target();
        if changed {
            fika_log!("[fika-wgpu] external-dnd clear=1");
        }
        changed
    }

    fn finish_external_drag(
        &mut self,
        sources: Vec<PathBuf>,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> Result<bool, String> {
        self.pointer = Some(point);
        let sources = normalized_external_drop_sources(sources);
        let drag_cleared = self.external_drag.take().is_some();
        let Some(target) =
            self.drop_target_at_screen_point_for_external_drag(point, size, &sources)
        else {
            let hover_cleared = self.clear_dnd_hover_target();
            return Ok(drag_cleared || hover_cleared);
        };
        let Some(target_dir) = self.target_dir_for_drop_target(&target) else {
            let hover_cleared = self.clear_dnd_hover_target();
            return Ok(drag_cleared || hover_cleared);
        };
        let old_menu = self.drop_menu.clone();
        self.drop_menu = Some(ShellDropMenu::new(sources, target_dir, target, point));
        self.context_menu = None;
        self.context_target = None;
        self.rubber_band = None;
        self.internal_drag = None;
        self.place_press = None;
        let _ = self.clear_dnd_hover_target();
        let changed = drag_cleared || old_menu != self.drop_menu;
        if changed {
            fika_log!(
                "[fika-wgpu] external-dnd-menu open=1 sources={} target={}",
                self.drop_menu
                    .as_ref()
                    .map(|menu| menu.sources.len())
                    .unwrap_or(0),
                self.drop_menu
                    .as_ref()
                    .map(|menu| menu.target.kind())
                    .unwrap_or("none")
            );
        }
        Ok(changed)
    }

    fn update_external_dnd_hover_target(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
    ) -> bool {
        let next = self.external_drag.as_ref().and_then(|drag| {
            self.drop_target_at_screen_point_for_external_drag(point, size, &drag.sources)
        });
        let changed = self.dnd_hover_target != next;
        if changed {
            self.dnd_hover_target = next;
            self.dnd_hover_changes += 1;
        }
        changed
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn clear_dnd_hover_target(&mut self) -> bool {
        let changed = self.dnd_hover_target.take().is_some();
        if changed {
            self.dnd_hover_changes += 1;
        }
        changed
    }

    fn pane_drag_paths_for_index(&self, pane_id: ShellPaneId, index: usize) -> Vec<PathBuf> {
        let Some(pane) = self.pane_view(pane_id) else {
            return Vec::new();
        };
        if pane.selection.contains(index) {
            let paths = self
                .pane_selection(pane_id)
                .into_iter()
                .flat_map(|selection| selection.selected.iter())
                .copied()
                .filter_map(|index| self.entry_path_for_pane_view(pane, index))
                .collect::<Vec<_>>();
            if !paths.is_empty() {
                return paths;
            }
        }
        self.entry_path_for_pane_view(pane, index)
            .into_iter()
            .collect()
    }

    fn pane_drag_source_for_index(
        &self,
        pane_id: ShellPaneId,
        index: usize,
    ) -> Option<(PathBuf, bool, String)> {
        let pane = self.pane_view(pane_id)?;
        let entry = pane.entries.get(index)?;
        let path = self.entry_path_for_pane_view(pane, index)?;
        Some((path, entry.is_dir, entry.name.as_ref().to_string()))
    }

    fn drag_label_for_paths(paths: &[PathBuf], fallback: String) -> String {
        if paths.len() > 1 {
            return format!("{} items", paths.len());
        }
        paths
            .first()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .unwrap_or(fallback)
    }

    fn begin_internal_drag_for_pane_item(
        &mut self,
        pane: ShellPaneId,
        index: usize,
        point: ViewPoint,
    ) -> bool {
        let Some((source_path, is_dir, fallback_label)) =
            self.pane_drag_source_for_index(pane, index)
        else {
            self.internal_drag = None;
            self.external_drag = None;
            return false;
        };
        let paths = self.pane_drag_paths_for_index(pane, index);
        if paths.is_empty() {
            self.internal_drag = None;
            self.external_drag = None;
            return false;
        }
        let label = Self::drag_label_for_paths(&paths, fallback_label);
        self.external_drag = None;
        self.internal_drag = Some(ShellInternalDrag::new(
            ShellInternalDragSource::PaneItem {
                pane,
                index,
                source_path,
                is_dir,
            },
            paths,
            label,
            point,
        ));
        true
    }

    fn begin_internal_drag_for_place(&mut self, index: usize, point: ViewPoint) -> bool {
        if !self.place_participates_in_dnd(index) {
            self.internal_drag = None;
            self.external_drag = None;
            return false;
        }
        let Some(place) = self.places.get(index) else {
            self.internal_drag = None;
            self.external_drag = None;
            return false;
        };
        self.external_drag = None;
        self.internal_drag = Some(ShellInternalDrag::new(
            ShellInternalDragSource::Place { index },
            vec![place.path.clone()],
            place.label.clone(),
            point,
        ));
        true
    }

    fn update_internal_drag(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        let drag_changed = {
            let Some(drag) = self.internal_drag.as_mut() else {
                return false;
            };
            drag.update(point)
        };
        if !self.internal_drag.as_ref().is_some_and(|drag| drag.active) {
            return drag_changed;
        }
        let hover_cleared = if self
            .internal_drag
            .as_ref()
            .and_then(ShellInternalDrag::source_place_index)
            .is_some()
        {
            let place_hover_cleared = if self.hovered_place.is_some() {
                self.set_hovered_place(None)
            } else {
                false
            };
            let item_hover_cleared = if self.hovered_item.is_some() {
                self.set_hovered_item(None)
            } else {
                false
            };
            place_hover_cleared || item_hover_cleared
        } else {
            false
        };
        let hover_changed = self.update_dnd_hover_target(point, size);
        drag_changed || hover_cleared || hover_changed
    }

    fn finish_internal_drag(&mut self, point: ViewPoint, size: PhysicalSize<u32>) -> bool {
        let user_places_path = default_user_places_path();
        match self.finish_internal_drag_with_user_places_path(point, size, &user_places_path) {
            Ok(changed) => changed,
            Err(error) => {
                fika_log!("[fika-wgpu] dnd-error {error}");
                false
            }
        }
    }

    fn finish_internal_drag_with_user_places_path(
        &mut self,
        point: ViewPoint,
        size: PhysicalSize<u32>,
        user_places_path: &Path,
    ) -> Result<bool, String> {
        let Some(drag) = self.internal_drag.take() else {
            return Ok(false);
        };
        if !drag.active {
            let _ = self.clear_dnd_hover_target();
            return Ok(false);
        }
        let Some(target) = self.drop_target_at_screen_point_for_drag(point, size, &drag) else {
            let _ = self.clear_dnd_hover_target();
            return Ok(false);
        };
        if let ShellDropTarget::PlacesGap { index } = target {
            let changed =
                self.drop_internal_drag_to_places_gap(drag, index, user_places_path, size)?;
            let _ = self.clear_dnd_hover_target();
            return Ok(changed);
        }
        let Some(target_dir) = self.target_dir_for_drop_target(&target) else {
            let _ = self.clear_dnd_hover_target();
            return Ok(false);
        };
        if drag.paths.iter().any(|source| source == &target_dir) {
            let _ = self.clear_dnd_hover_target();
            return Ok(false);
        }
        let old_menu = self.drop_menu.clone();
        self.drop_menu = Some(ShellDropMenu::new(drag.paths, target_dir, target, point));
        self.context_menu = None;
        self.context_target = None;
        self.rubber_band = None;
        let _ = self.clear_dnd_hover_target();
        let changed = old_menu != self.drop_menu;
        if changed {
            fika_log!(
                "[fika-wgpu] dnd-menu open=1 sources={} target={}",
                self.drop_menu
                    .as_ref()
                    .map(|menu| menu.sources.len())
                    .unwrap_or(0),
                self.drop_menu
                    .as_ref()
                    .map(|menu| menu.target.kind())
                    .unwrap_or("none")
            );
        }
        Ok(changed)
    }

    fn drop_internal_drag_to_places_gap(
        &mut self,
        drag: ShellInternalDrag,
        index: usize,
        user_places_path: &Path,
        size: PhysicalSize<u32>,
    ) -> Result<bool, String> {
        match drag.source {
            ShellInternalDragSource::Place {
                index: source_index,
            } => self.move_place_to_gap(source_index, index, user_places_path, size),
            ShellInternalDragSource::PaneItem {
                source_path,
                is_dir: true,
                ..
            } => self.add_pane_folder_to_places_gap(source_path, index, user_places_path, size),
            ShellInternalDragSource::PaneItem { .. } => Ok(false),
        }
    }

    fn move_place_to_gap(
        &mut self,
        source_index: usize,
        gap_index: usize,
        user_places_path: &Path,
        size: PhysicalSize<u32>,
    ) -> Result<bool, String> {
        if source_index >= self.places.len()
            || gap_index > self.places.len()
            || !self.place_participates_in_dnd(source_index)
            || !self.place_dnd_gap_index_is_valid(gap_index)
            || gap_index == source_index
            || gap_index == source_index.saturating_add(1)
        {
            return Ok(false);
        }
        let place = self.places.remove(source_index);
        let insert_index = if gap_index > source_index {
            gap_index.saturating_sub(1)
        } else {
            gap_index
        }
        .min(self.places.len());
        let path = place.path.clone();
        let label = place.label.clone();
        self.places.insert(insert_index, place);
        save_shell_place_order(user_places_path, &self.places)?;
        self.context_target = None;
        self.context_menu = None;
        self.drop_menu = None;
        self.properties_overlay = None;
        self.rubber_band = None;
        self.clamp_places_scroll(size);
        self.places_changes += 1;
        self.refresh_hover(size);
        fika_log!(
            "[fika-wgpu] places-reorder label={:?} path={} from={} gap={} to={} changes={}",
            label,
            path.display(),
            source_index,
            gap_index,
            insert_index,
            self.places_changes
        );
        Ok(true)
    }
}
