mod dropped;
mod edit;
mod entry;
mod ordering;
mod persistence;
mod removal;

use std::path::{Path, PathBuf};

use fika_core::PaneId;

use crate::FikaApp;
use crate::ui::place_draft::PlaceDraft;

use super::model::default_place_label;

impl FikaApp {
    pub(crate) fn user_places(&self) -> Vec<fika_core::UserPlace> {
        persistence::user_places(&self.places)
    }

    pub(crate) fn save_user_places(&self) -> Result<(), String> {
        fika_core::save_user_places(&self.user_places_path, &self.user_places())?;
        let place_order_path =
            fika_core::place_order_path_for_user_places_path(&self.user_places_path);
        fika_core::save_place_order(
            &place_order_path,
            &persistence::primary_place_order(&self.places),
        )
    }

    pub(crate) fn remove_place(&mut self, pane_id: PaneId, path: &Path) {
        let result = removal::remove_user_place(&mut self.places, path);
        let Some(removed_path) = result.removed_path() else {
            self.set_pane_status(pane_id, result.status_message());
            return;
        };
        if self
            .place_draft
            .as_ref()
            .and_then(|draft| draft.editing_path.as_deref())
            == Some(removed_path)
        {
            self.place_draft = None;
        }
        self.hidden_places.remove(removed_path);
        if let Err(error) = self.save_user_places() {
            self.set_pane_status(pane_id, error);
            return;
        }
        self.set_pane_status(pane_id, result.status_message());
    }

    pub(crate) fn insert_place_from_dropped_paths(
        &mut self,
        pane_id: PaneId,
        paths: Vec<PathBuf>,
        index: usize,
    ) {
        let result = dropped::add_user_place_from_dropped_paths(&mut self.places, &paths, index);
        let message = result.status_message();
        if !result.added() {
            self.set_pane_status(pane_id, message);
            return;
        }
        if let Err(error) = self.save_user_places() {
            self.set_pane_status(pane_id, error);
            return;
        }
        self.set_pane_status(pane_id, message);
    }

    pub(crate) fn move_user_place_to_insert_index(
        &mut self,
        pane_id: PaneId,
        source_index: usize,
        index: usize,
    ) {
        let label = match ordering::move_user_place_to_insert_index(
            &mut self.places,
            source_index,
            index,
        ) {
            ordering::MoveUserPlaceResult::Moved { label } => label,
            ordering::MoveUserPlaceResult::AlreadyThere => {
                self.set_pane_status(pane_id, "Place already there");
                return;
            }
            ordering::MoveUserPlaceResult::NotMovable => {
                self.set_pane_status(pane_id, "Place cannot be moved");
                return;
            }
        };
        if let Err(error) = self.save_user_places() {
            self.set_pane_status(pane_id, error);
            return;
        }
        self.set_pane_status(pane_id, format!("Moved place {label}"));
    }

    pub(crate) fn user_place_insert_index(&self, index: usize) -> usize {
        ordering::user_place_insert_index(&self.places, index)
    }

    pub(crate) fn commit_place_draft(&mut self) {
        let Some(draft) = self.place_draft.take() else {
            return;
        };
        let Some(current_dir) = self
            .panes
            .pane(draft.pane_id)
            .map(|pane| pane.current_dir.clone())
        else {
            return;
        };

        let result = edit::commit_user_place_draft(
            &mut self.places,
            &current_dir,
            &draft.label,
            &draft.path,
            draft.editing_path.as_deref(),
        );
        let message = result.status_message();
        if !result.changed() {
            self.set_pane_status(draft.pane_id, message);
            return;
        }

        if let Err(error) = self.save_user_places() {
            self.set_pane_status(draft.pane_id, error);
            return;
        }
        self.set_pane_status(draft.pane_id, message);
    }

    pub(crate) fn start_add_place(&mut self, pane_id: PaneId) {
        let Some(path) = self
            .panes
            .pane(pane_id)
            .map(|pane| pane.current_dir.clone())
        else {
            return;
        };
        self.panes.focus(pane_id);
        self.clear_rename_draft_for_pane(pane_id);
        self.clear_location_draft_for_pane(pane_id);
        self.place_draft = Some(PlaceDraft::for_add(
            pane_id,
            default_place_label(&path),
            &path,
        ));
        self.set_pane_status(pane_id, format!("Adding place {}", path.display()));
    }

    pub(crate) fn start_edit_place(&mut self, pane_id: PaneId, path: PathBuf) {
        let Some(place) = self
            .places
            .iter()
            .find(|place| place.path == path && place.editable)
            .cloned()
        else {
            self.set_pane_status(pane_id, "Place cannot be edited");
            return;
        };
        self.panes.focus(pane_id);
        self.clear_rename_draft_for_pane(pane_id);
        self.clear_location_draft_for_pane(pane_id);
        self.place_draft = Some(PlaceDraft::for_edit(pane_id, place.label, &place.path));
        self.set_pane_status(pane_id, "Editing place");
    }
}
